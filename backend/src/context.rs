use std::{
    env,
    fmt::Debug,
    fs::File,
    io::Write,
    mem,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use log::info;
#[cfg(test)]
use mockall::automock;
use opencv::core::{MatTraitConst, MatTraitConstManual, Vec4b};
use platforms::windows::{self, Capture, Error, Handle, KeyKind, KeyReceiver, Keys};
use strum::IntoEnumIterator;
use tokio::sync::broadcast;

use crate::{
    Action, ActionCondition, ActionKey, Bound, HotKeys, KeyBindingConfiguration, RequestHandler,
    RotatorMode,
    buff::{Buff, BuffKind, BuffState},
    database::{Configuration, KeyBinding, PotionMode},
    detect::{CachedDetector, Detector},
    mat::OwnedMat,
    minimap::{Minimap, MinimapState},
    player::{Player, PlayerState},
    poll_request,
    rotator::Rotator,
    skill::{Skill, SkillKind, SkillState},
};

// TODO: fix this later...
pub const ERDA_SHOWER_SKILL_POSITION: usize = 0;
pub const RUNE_BUFF_POSITION: usize = 0;
const FPS: u32 = 30;
pub const MS_PER_TICK: u64 = 1000 / FPS as u64;
const SAYRAM_ELIXIR_BUFF_POSITION: usize = 1;
const AURELIA_ELIXIR_BUFF_POSITION: usize = 2;
const EXP_X3_BUFF_POSITION: usize = 3;
const BONUS_EXP_BUFF_POSITION: usize = 4;
const LEGION_WEALTH_BUFF_POSITION: usize = 5;
const LEGION_LUCK_BUFF_POSITION: usize = 6;

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

/// Represents an object that can send keys
#[cfg_attr(test, automock)]
pub trait KeySender: Debug {
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

/// An object that stores the game information.
#[derive(Debug)]
pub struct Context {
    pub keys: &'static dyn KeySender,
    pub minimap: Minimap,
    pub player: Player,
    pub skills: [Skill; mem::variant_count::<SkillKind>()],
    pub buffs: [Buff; mem::variant_count::<BuffKind>()],
    pub halting: bool,
}

#[cfg(test)]
impl Default for Context {
    fn default() -> Self {
        Self {
            keys: Box::leak(Box::new(MockKeySender::new())),
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: [Skill::Detecting; mem::variant_count::<SkillKind>()],
            buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
            halting: false,
        }
    }
}

struct DefaultRequestHandler<'a> {
    context: &'a mut Context,
    config: &'a mut Configuration,
    hot_keys: &'a mut HotKeys,
    buffs: &'a mut Vec<(usize, KeyBinding)>,
    actions: &'a mut Vec<Action>,
    rotator: &'a mut Rotator,
    detector: &'a CachedDetector,
    player: &'a mut PlayerState,
    minimap: &'a mut MinimapState,
    key_sender: &'a broadcast::Sender<KeyBinding>,
}

impl DefaultRequestHandler<'_> {
    fn update_rotator_actions(&mut self, mode: RotatorMode) {
        self.rotator.build_actions(
            config_actions(self.config)
                .into_iter()
                .chain(self.actions.iter().copied())
                .collect::<Vec<_>>()
                .as_slice(),
            self.buffs,
            self.config.potion_key.key,
            mode,
        );
    }
}

impl RequestHandler for DefaultRequestHandler<'_> {
    fn on_rotate_actions(&mut self, halting: bool) {
        if self.minimap.data().is_some() {
            self.context.halting = halting;
            if halting {
                self.rotator.reset_queue();
                self.player.abort_actions();
            }
        }
    }

    fn on_rotate_actions_halting(&self) -> bool {
        self.context.halting
    }

    fn on_create_minimap(&self, name: String) -> Option<crate::Minimap> {
        if let Minimap::Idle(idle) = self.context.minimap {
            Some(crate::Minimap {
                name,
                width: idle.bbox.width,
                height: idle.bbox.height,
                ..crate::Minimap::default()
            })
        } else {
            None
        }
    }

    fn on_update_minimap(&mut self, preset: Option<String>, minimap: crate::Minimap) {
        self.minimap.set_data(minimap);

        let minimap = self.minimap.data().unwrap();
        self.player.reset();
        self.player.config.rune_platforms_pathing = minimap.rune_platforms_pathing;
        self.player.config.rune_platforms_pathing_up_jump_only =
            minimap.rune_platforms_pathing_up_jump_only;
        self.player.config.auto_mob_platforms_pathing = minimap.auto_mob_platforms_pathing;
        self.player.config.auto_mob_platforms_pathing_up_jump_only =
            minimap.auto_mob_platforms_pathing_up_jump_only;
        self.player.config.auto_mob_platforms_bound = minimap.auto_mob_platforms_bound;
        *self.actions = preset
            .and_then(|preset| minimap.actions.get(&preset).cloned())
            .unwrap_or_default();
        self.update_rotator_actions(minimap.rotation_mode.into());
    }

    fn on_update_configuration(&mut self, config: Configuration) {
        *self.config = config;
        *self.buffs = config_buffs(self.config);
        self.player.reset();
        self.player.config.class = self.config.class;
        self.player.config.interact_key = self.config.interact_key.key.into();
        self.player.config.grappling_key = self.config.ropelift_key.key.into();
        self.player.config.teleport_key = self.config.teleport_key.map(|key| key.key.into());
        self.player.config.upjump_key = self.config.up_jump_key.map(|key| key.key.into());
        self.player.config.cash_shop_key = self.config.cash_shop_key.key.into();
        self.player.config.potion_key = self.config.potion_key.key.into();
        self.player.config.use_potion_below_percent =
            match (self.config.potion_key.enabled, self.config.potion_mode) {
                (false, _) | (_, PotionMode::EveryMillis(_)) => None,
                (_, PotionMode::Percentage(percent)) => Some(percent / 100.0),
            };
        self.player.config.update_health_millis = Some(self.config.health_update_millis);
        self.update_rotator_actions(
            self.minimap
                .data()
                .map(|minimap| minimap.rotation_mode)
                .unwrap_or_default()
                .into(),
        );
    }

    fn on_update_hot_keys(&mut self, hot_keys: HotKeys) {
        *self.hot_keys = hot_keys;
    }

    fn on_redetect_minimap(&mut self) {
        self.context.minimap = Minimap::Detecting;
    }

    fn on_player_state(&self) -> crate::PlayerState {
        crate::PlayerState {
            position: self.player.last_known_pos.map(|pos| (pos.x, pos.y)),
            health: self.player.health,
            state: self.context.player.to_string(),
            normal_action: self.player.normal_action_name(),
            priority_action: self.player.priority_action_name(),
            erda_shower_state: self.context.skills[ERDA_SHOWER_SKILL_POSITION].to_string(),
            destinations: self
                .player
                .last_destinations
                .clone()
                .map(|points| {
                    points
                        .into_iter()
                        .map(|point| (point.x, point.y))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }
    }

    fn on_minimap_frame(&self) -> Option<(Vec<u8>, usize, usize)> {
        extract_minimap(self.context, self.detector.mat())
    }

    fn on_minimap_platforms_bound(&self) -> Option<Bound> {
        if let Minimap::Idle(idle) = self.context.minimap {
            idle.platforms_bound.map(|bound| bound.into())
        } else {
            None
        }
    }

    fn on_key_receiver(&self) -> broadcast::Receiver<KeyBinding> {
        self.key_sender.subscribe()
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
        windows::init();
        ort::init_from(dll.to_str().unwrap()).commit().unwrap();
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
    let handle = Handle::new("MapleStoryClass");
    let keys = DefaultKeySender {
        keys: Keys::new(handle),
    };
    let key_sender = broadcast::channel::<KeyBinding>(1).0; // Callback to UI
    let mut key_receiver = KeyReceiver::new(handle);
    let mut capture = Capture::new(handle);
    let mut player_state = PlayerState::default();
    let mut minimap_state = MinimapState::default();
    let mut skill_states = SkillKind::iter()
        .map(SkillState::new)
        .collect::<Vec<SkillState>>();
    let mut buff_states = BuffKind::iter()
        .map(BuffState::new)
        .collect::<Vec<BuffState>>();
    let mut rotator = Rotator::default();
    let mut actions = Vec::<Action>::new();
    let mut config = Configuration::default(); // Override by UI
    let mut buffs = config_buffs(&config);
    let mut context = Context {
        keys: Box::leak(Box::new(keys)),
        minimap: Minimap::Detecting,
        player: Player::Detecting,
        skills: [Skill::Detecting],
        buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
        halting: true,
    };
    let mut hot_keys = HotKeys::default(); // Override by UI

    loop_with_fps(FPS, || {
        let Ok(mat) = capture.grab().map(OwnedMat::new) else {
            return;
        };
        // I know what you are thinking...
        let detector = CachedDetector::new(mat);
        context.minimap = fold_context(&context, &detector, context.minimap, &mut minimap_state);
        context.player = fold_context(&context, &detector, context.player, &mut player_state);
        for (i, state) in skill_states
            .iter_mut()
            .enumerate()
            .take(context.skills.len())
        {
            context.skills[i] = fold_context(&context, &detector, context.skills[i], state);
        }
        for (i, state) in buff_states.iter_mut().enumerate().take(context.buffs.len()) {
            context.buffs[i] = fold_context(&context, &detector, context.buffs[i], state);
        }
        // rotating action must always be done last
        rotator.rotate_action(&context, &detector, &mut player_state);
        // I know what you are thinking...
        let mut handler = DefaultRequestHandler {
            context: &mut context,
            config: &mut config,
            hot_keys: &mut hot_keys,
            buffs: &mut buffs,
            actions: &mut actions,
            rotator: &mut rotator,
            detector: &detector,
            player: &mut player_state,
            minimap: &mut minimap_state,
            key_sender: &key_sender,
        };
        poll_request(&mut handler);
        poll_key(&mut handler, &mut key_receiver);
    });
}

#[inline]
fn poll_key(handler: &mut DefaultRequestHandler, receiver: &mut KeyReceiver) {
    let Some(received_key) = receiver.try_recv() else {
        return;
    };
    let KeyBindingConfiguration { key, enabled } = handler.hot_keys.toggle_actions_key;
    if enabled && KeyKind::from(key) == received_key {
        handler.on_rotate_actions(!handler.context.halting);
    }
    let _ = handler.key_sender.send(received_key.into());
}

#[inline]
fn extract_minimap(context: &Context, mat: &impl MatTraitConst) -> Option<(Vec<u8>, usize, usize)> {
    if let Minimap::Idle(idle) = context.minimap {
        let minimap = mat
            .roi(idle.bbox)
            .unwrap()
            .iter::<Vec4b>()
            .unwrap()
            .flat_map(|bgra| {
                let bgra = bgra.1;
                [bgra[2], bgra[1], bgra[0], 255]
            })
            .collect::<Vec<u8>>();
        return Some((minimap, idle.bbox.width as usize, idle.bbox.height as usize));
    }
    None
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

fn config_buffs(config: &Configuration) -> Vec<(usize, KeyBinding)> {
    let mut buffs = Vec::<(usize, KeyBinding)>::new();
    let KeyBindingConfiguration { key, enabled, .. } = config.sayram_elixir_key;
    if enabled {
        buffs.push((SAYRAM_ELIXIR_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled, .. } = config.aurelia_elixir_key;
    if enabled {
        buffs.push((AURELIA_ELIXIR_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled, .. } = config.exp_x3_key;
    if enabled {
        buffs.push((EXP_X3_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled, .. } = config.bonus_exp_key;
    if enabled {
        buffs.push((BONUS_EXP_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled, .. } = config.legion_luck_key;
    if enabled {
        buffs.push((LEGION_LUCK_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled, .. } = config.legion_wealth_key;
    if enabled {
        buffs.push((LEGION_WEALTH_BUFF_POSITION, key));
    }
    buffs
}

fn config_actions(config: &Configuration) -> Vec<Action> {
    let mut vec = Vec::new();
    let KeyBindingConfiguration { key, enabled } = config.feed_pet_key;
    if enabled {
        let feed_pet_action = Action::Key(ActionKey {
            key,
            count: 1,
            condition: ActionCondition::EveryMillis(config.feed_pet_millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        });
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
    }
    let KeyBindingConfiguration { key, enabled } = config.potion_key;
    if enabled && let PotionMode::EveryMillis(millis) = config.potion_mode {
        vec.push(Action::Key(ActionKey {
            key,
            count: 1,
            condition: ActionCondition::EveryMillis(millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        }));
    }
    vec
}
