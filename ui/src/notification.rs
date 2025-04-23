use backend::{Notifications as NotificationsData, Settings};
use dioxus::prelude::*;

use crate::{
    AppMessage,
    settings::{SettingsCheckbox, SettingsTextInput},
};

#[component]
pub fn Notifications(
    app_coroutine: Coroutine<AppMessage>,
    settings: ReadOnlySignal<Option<Settings>>,
) -> Element {
    let settings_view = use_memo(move || settings().unwrap_or_default());
    let notifications_view = use_memo(move || settings_view().notifications);
    let on_notifications = move |updated| {
        app_coroutine.send(AppMessage::UpdateSettings(Settings {
            notifications: updated,
            ..settings_view.peek().clone()
        }));
    };

    rsx! {
        div { class: "px-2 pb-2 pt-2 flex flex-col space-y-3 overflow-y-auto scrollbar h-full",
            SettingsTextInput {
                label: "Discord Webhook URL",
                on_input: move |discord_webhook_url| {
                    on_notifications(NotificationsData {
                        discord_webhook_url,
                        ..notifications_view.peek().clone()
                    });
                },
                value: notifications_view().discord_webhook_url,
            }
            SettingsTextInput {
                label: "Discord Ping User ID",
                on_input: move |discord_user_id| {
                    on_notifications(NotificationsData {
                        discord_user_id,
                        ..notifications_view.peek().clone()
                    });
                },
                value: notifications_view().discord_user_id,
            }
            SettingsCheckbox {
                label: "Notify If Fails / Changes Map",
                on_input: move |notify_on_fail_or_change_map| {
                    on_notifications(NotificationsData {
                        notify_on_fail_or_change_map,
                        ..notifications_view.peek().clone()
                    });
                },
                value: notifications_view().notify_on_fail_or_change_map,
            }
            SettingsCheckbox {
                label: "Notify If Rune Appears",
                on_input: move |notify_on_rune_appear| {
                    on_notifications(NotificationsData {
                        notify_on_rune_appear,
                        ..notifications_view.peek().clone()
                    });
                },
                value: notifications_view().notify_on_rune_appear,
            }
            SettingsCheckbox {
                label: "Notify If Player Dies",
                on_input: move |notify_on_player_die| {
                    on_notifications(NotificationsData {
                        notify_on_player_die,
                        ..notifications_view.peek().clone()
                    });
                },
                value: notifications_view().notify_on_player_die,
            }
        }
    }
}
