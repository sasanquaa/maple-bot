use std::{
    any::Any,
    cell::{RefCell, RefMut},
    env,
    fmt::Debug,
    fs::File,
    io::Write,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Error, anyhow};
use log::info;
#[cfg(test)]
use mockall::automock;
use opencv::{
    core::{Vector, VectorToVec},
    imgcodecs::imencode_def,
};
use platforms::windows::{
    self, BitBltCapture, Handle, KeyInputKind, KeyKind, KeyReceiver, Keys, WgcCapture,
    WindowBoxCapture,
};
use strum::IntoEnumIterator;
use tokio::sync::broadcast;

#[cfg(test)]
use crate::Settings;
use crate::{
    Action, RequestHandler,
    buff::{Buff, BuffKind, BuffState},
    database::{CaptureMode, InputMethod, KeyBinding},
    detect::{CachedDetector, Detector},
    mat::OwnedMat,
    minimap::{Minimap, MinimapState},
    network::{DiscordNotification, NotificationKind},
    player::{Player, PlayerState},
    query_configs, query_settings,
    request_handler::{DefaultRequestHandler, config_buffs},
    rotator::Rotator,
    rpc::KeysService,
    skill::{Skill, SkillKind, SkillState},
};

const FPS: u32 = 30;
pub const MS_PER_TICK: u64 = 1000 / FPS as u64;

/// Represents a control flow after a context update
pub enum ControlFlow<T> {
    /// The context is updated immediately
    Immediate(T),
    /// The context is updated in the next tick
    Next(T),
}

/// Represents a context-based state
pub trait Contextual {
    /// Represents a state that is persistent through each `update` tick.
    type Persistent = ();

    /// Updates the contextual state.
    ///
    /// This is basically a state machine.
    ///
    /// Updating is performed on each tick and the behavior whether to continue
    /// updating in the same tick or next is decided by `ControlFlow`. The state
    /// can transition or stay the same.
    fn update(
        self,
        context: &Context,
        detector: &impl Detector,
        persistent: &mut Self::Persistent,
    ) -> ControlFlow<Self>
    where
        Self: Sized;
}

/// The kind of key input method
///
/// Bridge enum between platform and RPC
#[derive(Clone, Debug)]
pub enum KeySenderKind {
    Rpc(String),
    Foreground(Handle),
    Fixed(Handle),
}

/// A trait for sending keys
#[cfg_attr(test, automock)]
pub trait KeySender: Debug + Any {
    fn set_kind(&mut self, kind: KeySenderKind);

    fn send(&self, kind: KeyKind) -> Result<(), Error>;

    fn send_click_to_focus(&self) -> Result<(), Error>;

    fn send_up(&self, kind: KeyKind) -> Result<(), Error>;

    fn send_down(&self, kind: KeyKind) -> Result<(), Error>;
}

#[derive(Debug)]
struct DefaultKeySender {
    platform: Keys,
    service: Option<RefCell<KeysService>>,
    kind: KeySenderKind,
}

impl DefaultKeySender {
    #[inline]
    fn borrow_service_mut(&self) -> Result<RefMut<KeysService>, Error> {
        self.service
            .as_ref()
            .map(|service| service.borrow_mut())
            .ok_or(anyhow!("service not connected"))
    }
}

impl KeySender for DefaultKeySender {
    fn set_kind(&mut self, kind: KeySenderKind) {
        match kind.clone() {
            KeySenderKind::Rpc(url) => {
                if let KeySenderKind::Rpc(ref cur_url) = self.kind
                    && cur_url != &url
                {
                    self.service = None;
                }
                if self.service.is_none() {
                    self.service = KeysService::connect(url).map(RefCell::new).ok();
                }
            }
            KeySenderKind::Foreground(handle) => {
                self.platform
                    .set_input_kind(handle, KeyInputKind::Foreground);
            }
            KeySenderKind::Fixed(handle) => {
                self.platform.set_input_kind(handle, KeyInputKind::Fixed);
            }
        }
        if let Ok(mut service) = self.borrow_service_mut() {
            service.reset();
        }
        self.kind = kind;
    }

    fn send(&self, kind: KeyKind) -> Result<(), Error> {
        match self.kind {
            KeySenderKind::Rpc(_) => {
                self.borrow_service_mut()?.send(kind)?;
                Ok(())
            }
            KeySenderKind::Foreground(_) | KeySenderKind::Fixed(_) => {
                self.platform.send(kind)?;
                Ok(())
            }
        }
    }

    fn send_click_to_focus(&self) -> Result<(), Error> {
        match self.kind {
            KeySenderKind::Rpc(_) => Ok(()),
            KeySenderKind::Foreground(_) | KeySenderKind::Fixed(_) => {
                self.platform.send_click_to_focus()?;
                Ok(())
            }
        }
    }

    fn send_up(&self, kind: KeyKind) -> Result<(), Error> {
        match self.kind {
            KeySenderKind::Rpc(_) => {
                self.borrow_service_mut()?.send_up(kind)?;
                Ok(())
            }
            KeySenderKind::Foreground(_) | KeySenderKind::Fixed(_) => {
                self.platform.send_up(kind)?;
                Ok(())
            }
        }
    }

    fn send_down(&self, kind: KeyKind) -> Result<(), Error> {
        match self.kind {
            KeySenderKind::Rpc(_) => {
                self.borrow_service_mut()?.send_down(kind)?;
                Ok(())
            }
            KeySenderKind::Foreground(_) | KeySenderKind::Fixed(_) => {
                self.platform.send_down(kind)?;
                Ok(())
            }
        }
    }
}

/// A struct that stores the game information
#[derive(Debug)]
pub struct Context {
    /// The `MapleStory` class game handle
    pub handle: Handle, // FIXME: This shoulnd't be pub, it is pub for tests
    pub keys: Box<dyn KeySender>,
    pub notification: DiscordNotification,
    pub minimap: Minimap,
    pub player: Player,
    pub skills: [Skill; SkillKind::COUNT],
    pub buffs: [Buff; BuffKind::COUNT],
    pub halting: bool,
}

#[cfg(test)]
impl Default for Context {
    fn default() -> Self {
        Self {
            handle: Handle::new(""),
            keys: Box::new(MockKeySender::new()),
            notification: DiscordNotification::new(Rc::new(RefCell::new(Settings::default()))),
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: [Skill::Detecting; SkillKind::COUNT],
            buffs: [Buff::NoBuff; BuffKind::COUNT],
            halting: false,
        }
    }
}

pub fn init() {
    static LOOPING: AtomicBool = AtomicBool::new(false);

    if LOOPING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire)
        .is_ok()
    {
        let dll = env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("onnxruntime.dll");
        if let Ok(mut file) = File::create_new(dll.clone()) {
            file.write_all(include_bytes!(env!("ONNX_RUNTIME")))
                .unwrap();
        }
        ort::init_from(dll.to_str().unwrap()).commit().unwrap();
        windows::init();
        thread::spawn(|| {
            let tokio_rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            let _tokio_guard = tokio_rt.enter();
            tokio_rt.block_on(async {
                update_loop();
            });
        });
    }
}

#[inline]
fn update_loop() {
    // MapleStoryClass <- GMS
    // MapleStoryClassSG <- MSEA
    // MapleStoryClassTW <- TMS
    let handle = Handle::new("MapleStoryClass");
    let mut rotator = Rotator::default();
    let mut actions = Vec::<Action>::new();
    let mut config = query_configs().unwrap().into_iter().next().unwrap(); // Override by UI
    let mut buffs = config_buffs(&config);
    let settings = query_settings(); // Override by UI

    let keys_service = if let InputMethod::Rpc = settings.input_method {
        KeysService::connect(settings.input_method_rpc_server_url.clone())
            .map(RefCell::new)
            .ok()
    } else {
        None
    };
    let key_sender_kind = if let InputMethod::Rpc = settings.input_method {
        KeySenderKind::Rpc(settings.input_method_rpc_server_url.clone())
    } else {
        match settings.capture_mode {
            CaptureMode::BitBlt | CaptureMode::WindowsGraphicsCapture => {
                KeySenderKind::Fixed(handle)
            }
            CaptureMode::BitBltArea => KeySenderKind::Foreground(handle),
        }
    };
    let mut keys = DefaultKeySender {
        platform: Keys::new(handle),
        service: keys_service,
        kind: key_sender_kind,
    };
    let key_sender = broadcast::channel::<KeyBinding>(1).0; // Callback to UI
    let mut key_receiver = KeyReceiver::new(handle, KeyInputKind::Fixed);

    let mut bitblt_capture = BitBltCapture::new(handle, false);
    let mut wgc_capture = WgcCapture::new(handle, MS_PER_TICK);
    let mut window_box_capture = WindowBoxCapture::default();
    if !matches!(settings.capture_mode, CaptureMode::BitBltArea) {
        window_box_capture.hide();
    } else {
        key_receiver = KeyReceiver::new(window_box_capture.handle(), KeyInputKind::Foreground);
        keys.platform
            .set_input_kind(window_box_capture.handle(), KeyInputKind::Foreground);
    }

    let settings = Rc::new(RefCell::new(settings));
    let mut context = Context {
        handle,
        keys: Box::new(keys),
        notification: DiscordNotification::new(settings.clone()),
        minimap: Minimap::Detecting,
        player: Player::Idle,
        skills: [Skill::Detecting],
        buffs: [Buff::NoBuff; BuffKind::COUNT],
        halting: true,
    };
    let mut player_state = PlayerState::default();
    let mut minimap_state = MinimapState::default();
    let mut skill_states = SkillKind::iter()
        .map(SkillState::new)
        .collect::<Vec<SkillState>>();
    let mut buff_states = BuffKind::iter()
        .map(BuffState::new)
        .collect::<Vec<BuffState>>();

    loop_with_fps(FPS, || {
        let mat = match settings.borrow().capture_mode {
            CaptureMode::BitBlt => bitblt_capture.grab().ok().map(OwnedMat::new),
            CaptureMode::WindowsGraphicsCapture => wgc_capture
                .as_mut()
                .ok()
                .and_then(|capture| capture.grab().ok())
                .map(OwnedMat::new),
            CaptureMode::BitBltArea => window_box_capture.grab().ok().map(OwnedMat::new),
        };
        let was_minimap_idle = matches!(context.minimap, Minimap::Idle(_));
        let detector = mat.map(CachedDetector::new);

        if let Some(ref detector) = detector {
            context.minimap = fold_context(&context, detector, context.minimap, &mut minimap_state);
            context.player = fold_context(&context, detector, context.player, &mut player_state);
            for (i, state) in skill_states
                .iter_mut()
                .enumerate()
                .take(context.skills.len())
            {
                context.skills[i] = fold_context(&context, detector, context.skills[i], state);
            }
            for (i, state) in buff_states.iter_mut().enumerate().take(context.buffs.len()) {
                context.buffs[i] = fold_context(&context, detector, context.buffs[i], state);
            }
            // Rotating action must always be done last
            rotator.rotate_action(&context, detector, &mut player_state);
        }

        // Poll requests, keys and update scheduled notifications frames
        let mut settings_borrow_mut = settings.borrow_mut();
        // I know what you are thinking...
        let mut handler = DefaultRequestHandler {
            context: &mut context,
            config: &mut config,
            settings: &mut settings_borrow_mut,
            buffs: &mut buffs,
            actions: &mut actions,
            rotator: &mut rotator,
            mat: detector.as_ref().map(|detector| detector.mat()),
            player: &mut player_state,
            minimap: &mut minimap_state,
            key_sender: &key_sender,
            key_receiver: &mut key_receiver,
            wgc_capture: wgc_capture.as_mut().ok(),
            window_box_capture: &window_box_capture,
        };
        handler.poll_request();
        handler.poll_key();
        handler
            .context
            .notification
            .update_scheduled_frames(|| to_png(handler.mat));
        // Upon accidental or white roomed causing map to change,
        // abort actions and send notification
        if handler.minimap.data().is_some()
            && matches!(handler.context.minimap, Minimap::Detecting)
            && was_minimap_idle
            && !handler.context.halting
        {
            if handler.settings.stop_on_fail_or_change_map {
                handler.on_rotate_actions(true);
            }
            drop(settings_borrow_mut); // For notification to borrow immutably
            let _ = context
                .notification
                .schedule_notification(NotificationKind::FailOrMapChanged);
        }
    });
}

#[inline]
fn fold_context<C>(
    context: &Context,
    detector: &impl Detector,
    contextual: C,
    persistent: &mut <C as Contextual>::Persistent,
) -> C
where
    C: Contextual,
{
    let mut control_flow = contextual.update(context, detector, persistent);
    loop {
        match control_flow {
            ControlFlow::Immediate(contextual) => {
                control_flow = contextual.update(context, detector, persistent);
            }
            ControlFlow::Next(contextual) => return contextual,
        }
    }
}

#[inline]
fn loop_with_fps(fps: u32, mut on_tick: impl FnMut()) {
    let nanos_per_frame = (1_000_000_000 / fps) as u128;
    loop {
        let start = Instant::now();

        on_tick();

        let now = Instant::now();
        let elapsed_nanos = now.duration_since(start).as_nanos();
        if elapsed_nanos <= nanos_per_frame {
            thread::sleep(Duration::new(0, (nanos_per_frame - elapsed_nanos) as u32));
        } else {
            info!(target: "context", "ticking running late at {}ms", (elapsed_nanos - nanos_per_frame) / 1_000_000);
        }
    }
}

#[inline]
fn to_png(frame: Option<&OwnedMat>) -> Option<Vec<u8>> {
    frame.and_then(|image| {
        let mut bytes = Vector::new();
        imencode_def(".png", image, &mut bytes).ok()?;
        Some(bytes.to_vec())
    })
}
