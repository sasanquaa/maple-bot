use std::{
    any::Any,
    env,
    fmt::Debug,
    fs::File,
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use log::info;
#[cfg(test)]
use mockall::automock;
use platforms::windows::{
    self, BitBltCapture, Error, Handle, KeyInputKind, KeyKind, KeyReceiver, Keys, WgcCapture,
    WindowBoxCapture,
};
use strum::IntoEnumIterator;
use tokio::sync::broadcast;

use crate::{
    Action,
    buff::{Buff, BuffKind, BuffState},
    database::{CaptureMode, KeyBinding},
    detect::{CachedDetector, Detector},
    mat::OwnedMat,
    minimap::{Minimap, MinimapState},
    player::{Player, PlayerState},
    query_configs, query_settings,
    request_handler::{DefaultRequestHandler, config_buffs},
    rotator::Rotator,
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

/// A trait for sending keys
#[cfg_attr(test, automock)]
pub trait KeySender: Debug + Any {
    fn set_input_kind(&mut self, handle: Handle, kind: KeyInputKind);

    fn send(&self, kind: KeyKind) -> Result<(), Error>;

    fn send_click_to_focus(&self) -> Result<(), Error>;

    fn send_up(&self, kind: KeyKind) -> Result<(), Error>;

    fn send_down(&self, kind: KeyKind) -> Result<(), Error>;
}

#[derive(Debug)]
struct DefaultKeySender {
    keys: Keys,
}

impl KeySender for DefaultKeySender {
    fn set_input_kind(&mut self, handle: Handle, kind: KeyInputKind) {
        self.keys.set_input_kind(handle, kind);
    }

    fn send(&self, kind: KeyKind) -> Result<(), Error> {
        self.keys.send(kind)
    }

    fn send_click_to_focus(&self) -> Result<(), Error> {
        self.keys.send_click_to_focus()
    }

    fn send_up(&self, kind: KeyKind) -> Result<(), Error> {
        self.keys.send_up(kind)
    }

    fn send_down(&self, kind: KeyKind) -> Result<(), Error> {
        self.keys.send_down(kind)
    }
}

/// A struct that stores the game information
#[derive(Debug)]
pub struct Context {
    /// The `MapleStory` class game handle
    pub handle: Handle, // FIXME: This shoulnd't be pub, it is pub for tests
    pub keys: &'static mut dyn KeySender,
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
            keys: Box::leak(Box::new(MockKeySender::new())),
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
                .enable_time()
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
    let keys = DefaultKeySender {
        keys: Keys::new(handle),
    };
    let key_sender = broadcast::channel::<KeyBinding>(1).0; // Callback to UI
    let mut key_receiver = KeyReceiver::new(handle, KeyInputKind::Fixed);

    let mut rotator = Rotator::default();
    let mut actions = Vec::<Action>::new();
    let mut config = query_configs().unwrap().into_iter().next().unwrap(); // Override by UI
    let mut buffs = config_buffs(&config);
    let mut context = Context {
        handle,
        keys: Box::leak(Box::new(keys)),
        minimap: Minimap::Detecting,
        player: Player::Idle,
        skills: [Skill::Detecting],
        buffs: [Buff::NoBuff; BuffKind::COUNT],
        halting: true,
    };
    let mut settings = query_settings(); // Override by UI

    let mut bitblt_capture = BitBltCapture::new(handle, false);
    let mut wgc_capture = WgcCapture::new(handle, MS_PER_TICK);
    let mut window_box_capture = WindowBoxCapture::default();
    if !matches!(settings.capture_mode, CaptureMode::BitBltArea) {
        window_box_capture.hide();
    } else {
        key_receiver = KeyReceiver::new(window_box_capture.handle(), KeyInputKind::Foreground);
        context
            .keys
            .set_input_kind(window_box_capture.handle(), KeyInputKind::Foreground);
    }

    let mut player_state = PlayerState::default();
    let mut minimap_state = MinimapState::default();
    let mut skill_states = SkillKind::iter()
        .map(SkillState::new)
        .collect::<Vec<SkillState>>();
    let mut buff_states = BuffKind::iter()
        .map(BuffState::new)
        .collect::<Vec<BuffState>>();

    loop_with_fps(FPS, || {
        let mat = match settings.capture_mode {
            CaptureMode::BitBlt => bitblt_capture.grab().ok().map(OwnedMat::new),
            CaptureMode::WindowsGraphicsCapture => wgc_capture
                .as_mut()
                .ok()
                .and_then(|capture| capture.grab().ok())
                .map(OwnedMat::new),
            CaptureMode::BitBltArea => window_box_capture.grab().ok().map(OwnedMat::new),
        };
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
        // I know what you are thinking...
        let mut handler = DefaultRequestHandler {
            context: &mut context,
            config: &mut config,
            settings: &mut settings,
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
