use std::{
    env,
    fs::File,
    io::Write,
    mem,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use log::info;
use opencv::core::{Mat, MatTraitConst, MatTraitConstManual, Vec4b};
use platforms::windows::{self, Capture, Handle, Keys};
use strum::IntoEnumIterator;

use crate::{
    Action, ActionCondition, ActionKey, KeyBindingConfiguration, Request,
    buff::{Buff, BuffKind, BuffState},
    database::{Configuration, KeyBinding, query_configs},
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

/// A struct that stores the current tick before timing out.
///
/// Most contextual state can be timed out as there is no guaranteed
/// an action will be performed or state can be transitioned. So timeout is used to retry
/// such action/state and to avoid looping in a single state forever. Or
/// for some contextual states to perform an action only after timing out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeout {
    pub current: u32,
    pub started: bool,
}

#[inline]
pub fn update_with_timeout<T, A>(
    timeout: Timeout,
    max_timeout: u32,
    args: A,
    on_started: impl FnOnce(A, Timeout) -> T,
    on_timeout: impl FnOnce(A) -> T,
    on_update: impl FnOnce(A, Timeout) -> T,
) -> T {
    debug_assert!(max_timeout > 0, "max_timeout must be positive");
    debug_assert!(
        timeout.started || timeout == Timeout::default(),
        "started timeout in non-default state"
    );
    debug_assert!(
        timeout.current <= max_timeout,
        "current timeout tick larger than max_timeout"
    );

    match timeout {
        Timeout { started: false, .. } => on_started(args, Timeout {
            started: true,
            ..timeout
        }),
        Timeout { current, .. } if current >= max_timeout => on_timeout(args),
        timeout => on_update(args, Timeout {
            current: timeout.current + 1,
            ..timeout
        }),
    }
}

/// Represents a control flow after a context update.
pub enum ControlFlow<T> {
    /// The context is updated immediately
    Immediate(T),
    /// The context is updated in the next tick
    Next(T),
}

/// Represents a context-based state.
pub trait Contextual {
    /// Represents a state that is persistent through each `update` tick.
    type Persistent = ();

    fn update(
        self,
        context: &Context,
        detector: &mut impl Detector,
        persistent: &mut Self::Persistent,
    ) -> ControlFlow<Self>
    where
        Self: Sized;
}

/// An object that stores the game information.
#[derive(Debug)]
pub struct Context {
    pub keys: Keys,
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
            keys: Keys::new(Handle::new(Some("Class"), Some("Title")).unwrap()),
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: [Skill::Detecting(Timeout::default()); mem::variant_count::<SkillKind>()],
            buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
            halting: true,
        }
    }
}

pub fn start_update_loop() {
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
            let handle = Handle::new(Some("MapleStoryClass"), None).unwrap();
            let keys = Keys::new(handle);
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
            let mut config = query_configs().unwrap().into_iter().next().unwrap();
            let mut buffs = config_buffs(&config);
            let mut context = Context {
                keys,
                minimap: Minimap::Detecting,
                player: Player::Detecting,
                skills: [Skill::Detecting(Timeout::default())],
                buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
                halting: true,
            };
            let mut ignore_update_action = false;
            let update_minimap = |updated_minimap: crate::database::Minimap,
                                  preset: Option<String>,
                                  config: &Configuration,
                                  buffs: &Vec<(usize, KeyBinding)>,
                                  minimap_state: &mut MinimapState,
                                  actions: &mut Vec<Action>,
                                  rotator: &mut Rotator| {
                minimap_state.data = updated_minimap;
                *actions = preset
                    .and_then(|preset| minimap_state.data.actions.get(&preset).cloned())
                    .unwrap_or_default();
                rotator.build_actions(
                    config_actions(config)
                        .into_iter()
                        .chain(actions.iter().copied())
                        .collect::<Vec<_>>()
                        .as_slice(),
                    buffs,
                    config.potion_key.key,
                );
            };
            let update_config = |updated_config: Configuration,
                                 config: &mut Configuration,
                                 buffs: &mut Vec<(usize, KeyBinding)>,
                                 actions: &Vec<Action>,
                                 player_state: &mut PlayerState,
                                 rotator: &mut Rotator| {
                *config = updated_config;
                *buffs = config_buffs(config);
                player_state.interact_key = config.interact_key.key.into();
                player_state.grappling_key = config.ropelift_key.key.into();
                player_state.upjump_key = config.up_jump_key.map(|key| key.key.into());
                player_state.cash_shop_key = config.cash_shop_key.key.into();
                rotator.rotator_mode(config.rotation_mode.into());
                rotator.build_actions(
                    config_actions(config)
                        .into_iter()
                        .chain(actions.iter().copied())
                        .collect::<Vec<_>>()
                        .as_slice(),
                    buffs,
                    config.potion_key.key,
                );
            };

            loop_with_fps(FPS, || {
                let Ok(mat) = capture.grab().map(OwnedMat::new) else {
                    return;
                };
                let mut detector = CachedDetector::new(&mat);
                context.minimap =
                    fold_context(&context, &mut detector, context.minimap, &mut minimap_state);
                context.player =
                    fold_context(&context, &mut detector, context.player, &mut player_state);
                (0..context.skills.len()).for_each(|i| {
                    context.skills[i] = fold_context(
                        &context,
                        &mut detector,
                        context.skills[i],
                        &mut skill_states[i],
                    );
                });
                (0..context.buffs.len()).for_each(|i| {
                    context.buffs[i] = fold_context(
                        &context,
                        &mut detector,
                        context.buffs[i],
                        &mut buff_states[i],
                    );
                });
                if !context.halting {
                    rotator.rotate_action(&context, &mut player_state);
                }
                poll_request(|request| match request {
                    Request::RotateActions(halted) => {
                        context.halting = halted;
                        if halted {
                            rotator.reset_queue();
                            player_state.abort_actions();
                        }
                        Box::new(())
                    }
                    Request::MinimapFrame => Box::new(extract_minimap(&context, &mat)),
                    Request::RedetectMinimap => {
                        context.minimap = Minimap::Detecting;
                        Box::new(())
                    }
                    Request::MinimapData => Box::new(
                        matches!(context.minimap, Minimap::Idle(_))
                            .then_some(minimap_state.data.clone()),
                    ),
                    Request::PlayerState => Box::new(crate::PlayerState {
                        position: player_state.last_known_pos.map(|pos| (pos.x, pos.y)),
                        state: context.player.to_string(),
                        normal_action: player_state.normal_action_name(),
                        priority_action: player_state.priority_action_name(),
                        erda_shower_state: context.skills[ERDA_SHOWER_SKILL_POSITION].to_string(),
                    }),
                    Request::UpdateMinimap(preset, updated_minimap) => {
                        if matches!(context.player, Player::CashShopThenExit(_, _, _)) {
                            ignore_update_action = true;
                        }
                        if !ignore_update_action {
                            update_minimap(
                                updated_minimap,
                                preset,
                                &config,
                                &buffs,
                                &mut minimap_state,
                                &mut actions,
                                &mut rotator,
                            );
                        }
                        if ignore_update_action && matches!(context.minimap, Minimap::Idle(_)) {
                            ignore_update_action = false;
                        }
                        Box::new(())
                    }
                    Request::UpdateConfiguration(updated_config) => {
                        if matches!(context.player, Player::CashShopThenExit(_, _, _)) {
                            ignore_update_action = true;
                        }
                        if !ignore_update_action {
                            update_config(
                                updated_config,
                                &mut config,
                                &mut buffs,
                                &mut actions,
                                &mut player_state,
                                &mut rotator,
                            );
                        }
                        if ignore_update_action && matches!(context.minimap, Minimap::Idle(_)) {
                            ignore_update_action = false;
                        }
                        Box::new(())
                    }
                });
            });
        });
    }
}

#[inline]
fn extract_minimap(context: &Context, mat: &Mat) -> Option<(Vec<u8>, usize, usize)> {
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
    detector: &mut impl Detector,
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
    let KeyBindingConfiguration {
        key,
        enabled,
        millis,
    } = config.feed_pet_key;
    if enabled {
        let feed_pet_action = Action::Key(ActionKey {
            key,
            condition: ActionCondition::EveryMillis(millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        });
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
    }
    let KeyBindingConfiguration {
        key,
        enabled,
        millis,
    } = config.potion_key;
    if enabled {
        vec.push(Action::Key(ActionKey {
            key,
            condition: ActionCondition::EveryMillis(millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        }));
    }
    vec
}
