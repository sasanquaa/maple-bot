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
use platforms::windows::{self, Capture, Handle, KeyKind, Keys};

use crate::{
    Action, ActionCondition, ActionKey, KeyBindingConfiguration, Request, RotationMode,
    buff::{Buff, BuffKind, BuffState},
    database::{Configuration, KeyBinding, query_configs},
    detect::{CachedDetector, Detector},
    mat::OwnedMat,
    minimap::{Minimap, MinimapState},
    player::{Player, PlayerState},
    poll_request,
    rotator::{Rotator, RotatorMode},
    skill::{Skill, SkillKind, SkillState},
};

pub const ERDA_SHOWER_SKILL_POSITION: usize = 0;
pub const RUNE_BUFF_POSITION: usize = 0;
const SAYRAM_ELIXIR_BUFF_POSITION: usize = 1;
const EXP_X3_BUFF_POSITION: usize = 2;
const BONUS_EXP_BUFF_POSITION: usize = 3;
const LEGION_WEALTH_BUFF_POSITION: usize = 4;
const LEGION_LUCK_BUFF_POSITION: usize = 5;

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
}

#[cfg(test)]
impl Default for Context {
    fn default() -> Self {
        Self {
            keys: Keys::new(Handle::new(Some("Class"), Some("Title")).unwrap()),
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: [Skill::Detecting; mem::variant_count::<SkillKind>()],
            buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
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
            let mut halting = true;
            let mut capture = Capture::new(handle);
            let mut player_state = PlayerState::default();
            let mut minimap_state = MinimapState::default();
            let mut skill_states = [SkillState::new(SkillKind::ErdaShower)];
            let mut buff_states = [
                BuffState::new(BuffKind::Rune),
                BuffState::new(BuffKind::SayramElixir),
                BuffState::new(BuffKind::ExpCouponX3),
                BuffState::new(BuffKind::BonusExpCoupon),
                BuffState::new(BuffKind::LegionWealth),
                BuffState::new(BuffKind::LegionLuck),
            ];
            let mut rotator = Rotator::default();
            let mut actions = Vec::<Action>::new();
            let mut config = query_configs().unwrap().into_iter().next().unwrap();
            let mut buffs = config_buffs(&config);
            let mut context = Context {
                keys,
                minimap: Minimap::Detecting,
                player: Player::Detecting,
                skills: [Skill::Detecting],
                buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
            };
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
                player_state.interact_key = map_key(config.interact_key.key);
                player_state.grappling_key = map_key(config.ropelift_key.key);
                player_state.upjump_key = config.up_jump_key.map(|key| key.key).map(map_key);
                player_state.cash_shop_key = map_key(config.cash_shop_key.key);
                rotator.rotator_mode(map_rotate_mode(config.rotation_mode));
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

            loop_with_fps(30, || {
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
                if !halting {
                    rotator.rotate_action(&context, &mut player_state);
                }
                poll_request(|request| match request {
                    Request::RotateActions(halted) => {
                        halting = halted;
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
                    Request::PlayerPosition => Box::new(player_state.last_known_pos),
                    Request::UpdateMinimap(preset, updated_minimap) => {
                        update_minimap(
                            updated_minimap,
                            preset,
                            &config,
                            &buffs,
                            &mut minimap_state,
                            &mut actions,
                            &mut rotator,
                        );
                        Box::new(())
                    }
                    Request::UpdateConfiguration(updated_config) => {
                        update_config(
                            updated_config,
                            &mut config,
                            &mut buffs,
                            &mut actions,
                            &mut player_state,
                            &mut rotator,
                        );
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

#[inline]
fn map_rotate_mode(mode: RotationMode) -> RotatorMode {
    match mode {
        RotationMode::StartToEnd => RotatorMode::StartToEnd,
        RotationMode::StartToEndThenReverse => RotatorMode::StartToEndThenReverse,
    }
}

#[inline]
pub fn map_key(key: KeyBinding) -> KeyKind {
    match key {
        KeyBinding::A => KeyKind::A,
        KeyBinding::B => KeyKind::B,
        KeyBinding::C => KeyKind::C,
        KeyBinding::D => KeyKind::D,
        KeyBinding::E => KeyKind::E,
        KeyBinding::F => KeyKind::F,
        KeyBinding::G => KeyKind::G,
        KeyBinding::H => KeyKind::H,
        KeyBinding::I => KeyKind::I,
        KeyBinding::J => KeyKind::J,
        KeyBinding::K => KeyKind::K,
        KeyBinding::L => KeyKind::L,
        KeyBinding::M => KeyKind::M,
        KeyBinding::N => KeyKind::N,
        KeyBinding::O => KeyKind::O,
        KeyBinding::P => KeyKind::P,
        KeyBinding::Q => KeyKind::Q,
        KeyBinding::R => KeyKind::R,
        KeyBinding::S => KeyKind::S,
        KeyBinding::T => KeyKind::T,
        KeyBinding::U => KeyKind::U,
        KeyBinding::V => KeyKind::V,
        KeyBinding::W => KeyKind::W,
        KeyBinding::X => KeyKind::X,
        KeyBinding::Y => KeyKind::Y,
        KeyBinding::Z => KeyKind::Z,
        KeyBinding::Zero => KeyKind::Zero,
        KeyBinding::One => KeyKind::One,
        KeyBinding::Two => KeyKind::Two,
        KeyBinding::Three => KeyKind::Three,
        KeyBinding::Four => KeyKind::Four,
        KeyBinding::Five => KeyKind::Five,
        KeyBinding::Six => KeyKind::Six,
        KeyBinding::Seven => KeyKind::Seven,
        KeyBinding::Eight => KeyKind::Eight,
        KeyBinding::Nine => KeyKind::Nine,
        KeyBinding::F1 => KeyKind::F1,
        KeyBinding::F2 => KeyKind::F2,
        KeyBinding::F3 => KeyKind::F3,
        KeyBinding::F4 => KeyKind::F4,
        KeyBinding::F5 => KeyKind::F5,
        KeyBinding::F6 => KeyKind::F6,
        KeyBinding::F7 => KeyKind::F7,
        KeyBinding::F8 => KeyKind::F8,
        KeyBinding::F9 => KeyKind::F9,
        KeyBinding::F10 => KeyKind::F10,
        KeyBinding::F11 => KeyKind::F11,
        KeyBinding::F12 => KeyKind::F12,
        KeyBinding::Up => KeyKind::Up,
        KeyBinding::Home => KeyKind::Home,
        KeyBinding::End => KeyKind::End,
        KeyBinding::PageUp => KeyKind::PageUp,
        KeyBinding::PageDown => KeyKind::PageDown,
        KeyBinding::Insert => KeyKind::Insert,
        KeyBinding::Delete => KeyKind::Delete,
        KeyBinding::Enter => KeyKind::Enter,
        KeyBinding::Space => KeyKind::Space,
        KeyBinding::Tilde => KeyKind::Tilde,
        KeyBinding::Quote => KeyKind::Quote,
        KeyBinding::Semicolon => KeyKind::Semicolon,
        KeyBinding::Comma => KeyKind::Comma,
        KeyBinding::Period => KeyKind::Period,
        KeyBinding::Slash => KeyKind::Slash,
        KeyBinding::Esc => KeyKind::Esc,
        KeyBinding::Shift => KeyKind::Shift,
        KeyBinding::Ctrl => KeyKind::Ctrl,
        KeyBinding::Alt => KeyKind::Alt,
    }
}

fn config_buffs(config: &Configuration) -> Vec<(usize, KeyBinding)> {
    let mut buffs = Vec::<(usize, KeyBinding)>::new();
    let KeyBindingConfiguration { key, enabled } = config.sayram_elixir_key;
    if enabled {
        buffs.push((SAYRAM_ELIXIR_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled } = config.exp_x3_key;
    if enabled {
        buffs.push((EXP_X3_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled } = config.bonus_exp_key;
    if enabled {
        buffs.push((BONUS_EXP_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled } = config.legion_luck_key;
    if enabled {
        buffs.push((LEGION_LUCK_BUFF_POSITION, key));
    }
    let KeyBindingConfiguration { key, enabled } = config.legion_wealth_key;
    if enabled {
        buffs.push((LEGION_WEALTH_BUFF_POSITION, key));
    }
    buffs
}

fn config_actions(config: &Configuration) -> Vec<Action> {
    let mut vec = Vec::new();
    if config.feed_pet_key.enabled {
        let feed_pet_action = Action::Key(ActionKey {
            key: config.feed_pet_key.key,
            condition: ActionCondition::EveryMillis(120000),
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 10,
            ..ActionKey::default()
        });
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
    }
    if config.potion_key.enabled {
        vec.push(Action::Key(ActionKey {
            key: config.potion_key.key,
            condition: ActionCondition::EveryMillis(120000),
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 10,
            ..ActionKey::default()
        }));
    }
    vec
}
