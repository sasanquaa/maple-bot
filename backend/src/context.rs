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
use platforms::windows::{
    self,
    capture::DynamicCapture,
    handle::Handle,
    keys::{KeyKind, Keys},
};

use crate::{
    Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, Request, RotationMode,
    buff::{Buff, BuffKind, BuffState},
    database::{Configuration, KeyBinding, delete_map, query_config, refresh_config, refresh_map},
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
const LEGION_WEALTH_BUFF_POSITION: usize = 3;
const LEGION_LUCK_BUFF_POSITION: usize = 4;

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
        mat: &Mat,
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
            let mut capture = DynamicCapture::new(handle).unwrap();
            let mut player_state = PlayerState::default();
            let mut minimap_state = MinimapState::default();
            let mut skill_states = [SkillState::new(SkillKind::ErdaShower)];
            let mut buff_states = [
                BuffState::new(BuffKind::Rune),
                BuffState::new(BuffKind::SayramElixir),
                BuffState::new(BuffKind::ExpCouponX3),
                BuffState::new(BuffKind::LegionWealth),
                BuffState::new(BuffKind::LegionLuck),
            ];
            let mut rotator = Rotator::default();
            let mut actions = Vec::<Action>::new();
            let mut config = query_config().unwrap();
            let mut buffs = default_buffs(&config);
            let mut context = Context {
                keys,
                minimap: Minimap::Detecting,
                player: Player::Detecting,
                skills: [Skill::Detecting],
                buffs: [Buff::NoBuff; mem::variant_count::<BuffKind>()],
            };

            rotator.rotator_mode(map_rotate_mode(config.rotation_mode));

            loop_with_fps(30, || {
                let Ok(mat) = capture.grab().map(OwnedMat::new) else {
                    return;
                };
                context.minimap = fold_context(&context, &mat, context.minimap, &mut minimap_state);
                context.player = fold_context(&context, &mat, context.player, &mut player_state);
                (0..context.skills.len()).for_each(|i| {
                    context.skills[i] =
                        fold_context(&context, &mat, context.skills[i], &mut skill_states[i]);
                });
                (0..context.buffs.len()).for_each(|i| {
                    context.buffs[i] =
                        fold_context(&context, &mat, context.buffs[i], &mut buff_states[i]);
                });
                if !halting {
                    rotator.rotate_action(&context, &mut player_state);
                }
                poll_request(|request| match request {
                    Request::RotateActions(halted) => {
                        halting = halted;
                        if halted {
                            rotator.reset();
                            player_state.abort_actions();
                        }
                        Box::new(())
                    }
                    Request::PrepareActions(preset) => {
                        if let Some(preset_actions) = minimap_state.data.actions.get(&preset) {
                            if matches!(context.minimap, Minimap::Idle(_)) {
                                actions = preset_actions.clone();
                                rotator.build_actions(
                                    &[&default_actions(&config), actions.as_slice()]
                                        .concat()
                                        .to_vec(),
                                    &buffs,
                                );
                                return Box::new(true);
                            }
                        }
                        Box::new(false)
                    }
                    Request::MinimapFrame => Box::new(extract_minimap(&context, &mat)),
                    Request::RedetectMinimap(delete) => {
                        context.minimap = Minimap::Detecting;
                        if delete {
                            let _ = delete_map(&minimap_state.data);
                        }
                        Box::new(())
                    }
                    Request::MinimapData => Box::new(
                        matches!(context.minimap, Minimap::Idle(_))
                            .then_some(minimap_state.data.clone()),
                    ),
                    Request::PlayerPosition => Box::new(player_state.last_known_pos),
                    Request::RefreshMinimapData => {
                        let _ = refresh_map(&mut minimap_state.data);
                        Box::new(())
                    }
                    Request::RefreshConfiguration => {
                        let _ = refresh_config(&mut config);
                        player_state.interact_key = Some(map_key(config.interact_key));
                        player_state.grappling_key = Some(map_key(config.ropelift_key));
                        player_state.upjump_key = config.up_jump_key.map(map_key);
                        buffs = default_buffs(&config);
                        rotator.rotator_mode(map_rotate_mode(config.rotation_mode));
                        rotator.build_actions(
                            &[&default_actions(&config), actions.as_slice()]
                                .concat()
                                .to_vec(),
                            &buffs,
                        );
                        Box::new(())
                    }
                });
            });
        });
    }
}

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

fn fold_context<C>(
    context: &Context,
    mat: &Mat,
    contextual: C,
    persistent: &mut <C as Contextual>::Persistent,
) -> C
where
    C: Contextual,
{
    let mut control_flow = contextual.update(context, mat, persistent);
    loop {
        match control_flow {
            ControlFlow::Immediate(contextual) => {
                control_flow = contextual.update(context, mat, persistent);
            }
            ControlFlow::Next(contextual) => return contextual,
        }
    }
}

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

fn map_rotate_mode(mode: RotationMode) -> RotatorMode {
    match mode {
        RotationMode::StartToEnd => RotatorMode::StartToEnd,
        RotationMode::StartToEndThenReverse => RotatorMode::StartToEndThenReverse,
    }
}

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
        KeyBinding::Esc => KeyKind::Esc,
        KeyBinding::Shift => KeyKind::Shift,
        KeyBinding::Ctrl => KeyKind::Ctrl,
        KeyBinding::Alt => KeyKind::Alt,
    }
}

fn default_buffs(config: &Configuration) -> Vec<(usize, KeyBinding)> {
    let mut buffs = Vec::<(usize, KeyBinding)>::new();
    if let Some(key) = config.sayram_elixir_key {
        buffs.push((SAYRAM_ELIXIR_BUFF_POSITION, key));
    }
    if let Some(key) = config.exp_x3_key {
        buffs.push((EXP_X3_BUFF_POSITION, key));
    }
    if let Some(key) = config.legion_luck_key {
        buffs.push((LEGION_LUCK_BUFF_POSITION, key));
    }
    if let Some(key) = config.legion_wealth_key {
        buffs.push((LEGION_WEALTH_BUFF_POSITION, key));
    }
    buffs
}

fn default_actions(config: &Configuration) -> [Action; 4] {
    let feed_pet_action = ActionKey {
        key: config.feed_pet_key,
        position: None,
        condition: ActionCondition::EveryMillis(120000),
        direction: ActionKeyDirection::Any,
        with: ActionKeyWith::Any,
        wait_before_use_ticks: 10,
        wait_after_use_ticks: 10,
    };
    let potion_action = ActionKey {
        key: config.potion_key,
        ..feed_pet_action
    };
    [
        Action::Key(feed_pet_action),
        Action::Key(feed_pet_action),
        Action::Key(feed_pet_action),
        Action::Key(potion_action),
    ]
}
