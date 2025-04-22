use std::{
    cell::RefCell,
    mem,
    ops::{Index, Not},
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Error, Ok, bail};
use bit_vec::BitVec;
use log::{debug, error};
use reqwest::{
    Client, Url,
    multipart::{Form, Part},
};
use serde::Serialize;
use tokio::{
    spawn,
    time::{Instant, sleep},
};

use crate::Settings;

static TRUE: bool = true;
static FALSE: bool = false;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[repr(usize)]
pub enum NotificationKind {
    FailOrMapChanged,
    RuneAppear,
}

impl From<NotificationKind> for usize {
    fn from(kind: NotificationKind) -> Self {
        kind as usize
    }
}

impl Index<NotificationKind> for BitVec {
    type Output = bool;

    fn index(&self, index: NotificationKind) -> &Self::Output {
        if self.get(index.into()).expect("index out of bound") {
            &TRUE
        } else {
            &FALSE
        }
    }
}

#[derive(Debug)]
struct ScheduledNotification {
    /// The instant it was scheduled
    instant: Instant,
    kind: NotificationKind,
    url: String,
    body: DiscordWebhookBody,
    /// Stores fixed size tuples of frame and frame deadline in seconds
    ///
    /// During each [`DiscordNotification::update_schedule`], the first frame not passing the
    /// deadline will try to capture the image from current game state. This is useful for showing
    /// `before and after` whnen map changes. So frame that cannot capture when the deadline is
    /// reached will be skipped.
    frames: Vec<(Option<Vec<u8>>, u32)>,
}

#[derive(Debug)]
pub struct DiscordNotification {
    client: Client,
    settings: Rc<RefCell<Settings>>,
    scheduled: Arc<Mutex<Vec<ScheduledNotification>>>,
    /// Storing currently incomplete / pending notifications
    ///
    /// There can only be one unique [`NotificationKind`] scheduled at a time.
    pending: Arc<Mutex<BitVec>>,
}

impl DiscordNotification {
    pub fn new(settings: Rc<RefCell<Settings>>) -> Self {
        Self {
            client: Client::new(),
            settings,
            scheduled: Arc::new(Mutex::new(vec![])),
            pending: Arc::new(Mutex::new(BitVec::from_elem(
                mem::variant_count::<NotificationKind>(),
                false,
            ))),
        }
    }

    pub fn schedule_notification(&self, kind: NotificationKind) -> Result<(), Error> {
        let settings = self.settings.borrow();
        let is_enabled = match kind {
            NotificationKind::FailOrMapChanged => {
                settings.notifications.notify_on_fail_or_change_map
            }
            NotificationKind::RuneAppear => settings.notifications.notify_on_rune_appear,
        };
        if !is_enabled {
            bail!("notification not enabled");
        }
        if settings.notifications.discord_webhook_url.is_empty() {
            bail!("webhook url not provided");
        }

        let mut pending = self.pending.lock().unwrap();
        if pending[kind] {
            bail!("notification is already sending");
        }

        let url = settings.notifications.discord_webhook_url.clone();
        if Url::try_from(url.as_str()).is_err() {
            bail!("failed to parse webhook url");
        }

        let user_id = settings
            .notifications
            .discord_user_id
            .is_empty()
            .not()
            .then_some(format!("<@{}> ", settings.notifications.discord_user_id))
            .unwrap_or_default();
        let content = match kind {
            NotificationKind::FailOrMapChanged => {
                if self.settings.borrow().stop_on_fail_or_change_map {
                    format!(
                        "{user_id}Bot stopped because it has failed to detect or the map has changed"
                    )
                } else {
                    format!("{user_id}Bot has failed to detect or the map has changed")
                }
            }
            NotificationKind::RuneAppear => {
                format!("{user_id}Bot has detected a rune on map")
            }
        };
        let body = DiscordWebhookBody {
            content,
            username: "maple-bot",
            attachments: vec![],
        };
        let frames = match kind {
            NotificationKind::FailOrMapChanged => vec![(None, 3), (None, 6)],
            NotificationKind::RuneAppear => vec![(None, 3)],
        };

        let mut scheduled = self.scheduled.lock().unwrap();
        scheduled.push(ScheduledNotification {
            instant: Instant::now(),
            kind,
            url,
            frames,
            body,
        });
        pending.set(kind.into(), true);

        let client = self.client.clone();
        let pending = self.pending.clone();
        let scheduled = self.scheduled.clone();
        spawn(async move {
            sleep(Duration::from_secs(5)).await;

            let notification = scheduled
                .lock()
                .ok()
                .map(|mut scheduled| {
                    // Inside closure or compiler will complain about MutexGuard not being Send
                    let (index, _) = scheduled
                        .iter()
                        .enumerate()
                        .find(|(_, item)| item.kind == kind)
                        .unwrap();
                    scheduled.remove(index)
                })
                .unwrap();
            let kind = notification.kind;
            debug_assert!(
                pending
                    .lock()
                    .unwrap()
                    .get(notification.kind.into())
                    .unwrap()
            );
            pending.lock().unwrap().set(kind.into(), false);
            let _ = post_notification(client, notification).await;
        });

        Ok(())
    }

    pub fn update_scheduled_frames(&self, frame: impl Fn() -> Option<Vec<u8>>) {
        for item in self.scheduled.lock().unwrap().iter_mut() {
            let elapsed_secs = item.instant.elapsed().as_secs() as u32;
            for (item_frame, deadline) in item.frames.iter_mut() {
                if elapsed_secs <= *deadline {
                    if item_frame.is_none() {
                        *item_frame = frame();
                    }
                    break;
                }
            }
        }
    }
}

async fn post_notification(
    client: Client,
    mut notification: ScheduledNotification,
) -> Result<(), Error> {
    for i in 0..notification
        .frames
        .iter()
        .filter(|(frame, _)| frame.is_some())
        .count()
    {
        notification.body.attachments.push(Attachment {
            id: i,
            description: format!("Game snapshot #{i}"),
            filename: format!("image_{i}.png"),
        });
    }

    let mut form = Form::new().text(
        "payload_json",
        serde_json::to_string(&notification.body).unwrap(),
    );
    for (i, frame) in notification
        .frames
        .into_iter()
        .filter_map(|(frame, _)| frame)
        .enumerate()
    {
        form = form.part(
            format!("files[{i}]"),
            Part::bytes(frame)
                .mime_str("image/png")
                .unwrap()
                .file_name(format!("image_{i}.png")),
        );
    }

    let _ = client
        .post(notification.url)
        .multipart(form)
        .send()
        .await
        .inspect(|_| {
            debug!(target: "notification", "calling Webhook API {:?} succeeded", notification.kind);
        })
        .inspect_err(|err| {
            error!(target: "notification", "calling Webhook API failed {err}");
        });

    Ok(())
}

#[derive(Serialize, Debug)]
struct DiscordWebhookBody {
    content: String,
    username: &'static str,
    attachments: Vec<Attachment>,
}

#[derive(Serialize, Debug)]
struct Attachment {
    id: usize,
    description: String,
    filename: String,
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc, time::Duration};

    use tokio::time::{Instant, advance};

    use super::{DiscordNotification, DiscordWebhookBody, NotificationKind, ScheduledNotification};
    use crate::{Notifications, Settings};

    #[tokio::test(start_paused = true)]
    async fn schedule_kind_unique() {
        let noti = DiscordNotification::new(Rc::new(RefCell::new(Settings {
            notifications: Notifications {
                discord_webhook_url: "https://discord.com/api/webhooks/foo/bar".to_string(),
                notify_on_fail_or_change_map: true,
                notify_on_rune_appear: true,
                ..Default::default()
            },
            ..Default::default()
        })));

        assert!(
            noti.schedule_notification(NotificationKind::FailOrMapChanged)
                .is_ok()
        );
        assert!(noti.scheduled.lock().unwrap().len() == 1);
        assert!(
            noti.pending
                .lock()
                .unwrap()
                .get(NotificationKind::FailOrMapChanged.into())
                .unwrap()
        );
        assert!(
            noti.schedule_notification(NotificationKind::FailOrMapChanged)
                .is_err()
        );
        assert!(
            noti.schedule_notification(NotificationKind::RuneAppear)
                .is_ok()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_invalid_url() {
        let noti = DiscordNotification::new(Rc::new(RefCell::new(Settings {
            notifications: Notifications {
                notify_on_fail_or_change_map: true,
                ..Default::default()
            },
            ..Default::default()
        })));

        assert!(
            noti.schedule_notification(NotificationKind::FailOrMapChanged)
                .is_err()
        );
    }

    #[tokio::test(start_paused = true)]
    #[allow(clippy::await_holding_lock)]
    async fn update_scheduled_frames_deadline() {
        let noti = DiscordNotification::new(Rc::new(RefCell::new(Settings::default())));
        noti.scheduled.lock().unwrap().push(ScheduledNotification {
            instant: Instant::now(),
            kind: NotificationKind::FailOrMapChanged,
            url: "https://example.com".into(),
            frames: vec![(None, 3), (None, 6), (None, 9)],
            body: DiscordWebhookBody {
                content: "content".into(),
                username: "username",
                attachments: vec![],
            },
        });

        advance(Duration::from_secs(4)).await;
        // Skip frame 1 because deadline passed to frame 2
        noti.update_scheduled_frames(|| Some(vec![]));
        let scheduled_guard = noti.scheduled.lock().unwrap();
        let scheduled = scheduled_guard.first().unwrap();
        assert!(scheduled.frames[0].0.is_none());
        assert!(scheduled.frames[1].0.is_some());
        assert!(scheduled.frames[2].0.is_none());
        drop(scheduled_guard);

        // Frame 3
        advance(Duration::from_secs(4)).await;
        noti.update_scheduled_frames(|| Some(vec![]));
        let scheduled = noti.scheduled.lock().unwrap();
        let scheduled = scheduled.first().unwrap();
        assert!(scheduled.frames[0].0.is_none());
        assert!(scheduled.frames[1].0.is_some());
        assert!(scheduled.frames[2].0.is_some());
    }
}
