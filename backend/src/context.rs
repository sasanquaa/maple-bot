use std::{
    any::Any,
    collections::VecDeque,
    env,
    fs::File,
    io::Write,
    mem,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use log::debug;
use opencv::core::{Mat, MatTraitConst, MatTraitConstManual, Vec4b};
use platforms::windows::{
    self,
    capture::DynamicCapture,
    handle::Handle,
    keys::{KeyKind, Keys},
};
use tokio::{
    sync::oneshot::{self, Sender},
    time::timeout,
};

use crate::{
    mat::OwnedMat,
    minimap::{Minimap, MinimapState},
    models::{Action, ActionCondition, ActionKeyDirection, ActionKeyWith, KeyBinding, Position},
    player::{Player, PlayerState},
    rotator::Rotator,
    skill::{Skill, SkillKind, SkillState},
};

type RequestItem = (Sender<Box<dyn Any + Send>>, Request);

static REQUESTS: LazyLock<Mutex<VecDeque<RequestItem>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
pub(crate) const ERDA_SHOWER_SKILL_POSITION: usize = 0;

#[derive(Clone, Copy, Debug)]
enum Request {
    RotateActions,
    MinimapFrame,
}

/// Represents a control flow after a context update.
pub(crate) enum ControlFlow<T> {
    /// The context is updated immediately
    Immediate(T),
    /// The context is updated in the next tick
    Next(T),
}

pub(crate) trait Contextual {
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

#[derive(Debug)]
pub(crate) struct Context {
    pub(crate) keys: Keys,
    pub(crate) minimap: Minimap,
    pub(crate) player: Player,
    pub(crate) skills: [Skill; mem::variant_count::<SkillKind>()],
}

pub async fn request_minimap_frame() -> Result<Box<(Vec<u8>, usize, usize)>> {
    request(Request::MinimapFrame)
        .await
        .map(|boxed| unsafe { boxed.downcast_unchecked::<(Vec<u8>, usize, usize)>() })
}

async fn request(request: Request) -> Result<Box<dyn Any + Send>> {
    let (tx, rx) = oneshot::channel();
    REQUESTS.lock().unwrap().push_back((tx, request));
    let Ok(result) = timeout(Duration::from_secs(10), rx).await else {
        return Err(anyhow!("request timed out"));
    };
    result.map_err(|e| anyhow!("request error {e}"))
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
            let keys = Keys::new(handle.clone());
            let mut capture = DynamicCapture::new(handle.clone()).unwrap();
            let mut player_state = PlayerState::default();
            let mut minimap_state = MinimapState::default();
            let mut skill_states = [SkillState::new(SkillKind::ErdaShower)];
            let mut rotator = Rotator::default();
            let mut context = Context {
                keys,
                minimap: Minimap::Detecting,
                player: Player::Detecting,
                skills: [Skill::Detecting],
            };

            rotator.build_actions(&populate_actions());
            player_state.grappling_key = Some(KeyKind::F);
            player_state.upjump_key = Some(KeyKind::C);

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
                rotator.rotate_action(&context, &mut player_state);
                if let Some((sender, request)) = REQUESTS.lock().unwrap().pop_front() {
                    match request {
                        Request::RotateActions => todo!(),
                        Request::MinimapFrame => {
                            if let Some(frame) = extract_minimap(&context, &mat) {
                                let _ = sender.send(Box::new(frame));
                            }
                        }
                    }
                }
            });
        })
        .join()
        .unwrap();
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
            debug!(target: "context", "ticking running late at {}ms", (elapsed_nanos - nanos_per_frame) / 1_000_000);
        }
    }
}

fn populate_actions() -> Vec<Action> {
    vec![
        // Potion
        Action::Key {
            position: None,
            key: KeyBinding::One,
            condition: ActionCondition::EveryMillis(120000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 5,
        },
        // Feed pets
        Action::Key {
            position: None,
            key: KeyBinding::F7,
            condition: ActionCondition::EveryMillis(120000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 5,
        },
        Action::Key {
            position: None,
            key: KeyBinding::F7,
            condition: ActionCondition::EveryMillis(120000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 5,
        },
        Action::Key {
            position: None,
            key: KeyBinding::F7,
            condition: ActionCondition::EveryMillis(120000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 5,
        },
        // Scurvy Summons
        Action::Key {
            position: None,
            key: KeyBinding::Delete,
            condition: ActionCondition::EveryMillis(90000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 30,
            wait_after_use_ticks: 30,
        },
        // Roll of the Dice
        Action::Key {
            position: None,
            key: KeyBinding::F2,
            condition: ActionCondition::EveryMillis(240000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 30,
            wait_after_use_ticks: 30,
        },
        // Loot
        Action::Move {
            position: Position {
                x: 181,
                y: 27,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 3,
        },
        // Erda shower
        Action::Key {
            position: Some(Position {
                x: 168,
                y: 13,
                allow_adjusting: false,
            }),
            key: KeyBinding::Y,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 10,
        },
        // Trigger Erda Shower
        Action::Key {
            position: None,
            key: KeyBinding::A,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 3,
        },
        // Second portal
        Action::Key {
            position: Some(Position {
                x: 168,
                y: 13,
                allow_adjusting: true,
            }),
            key: KeyBinding::Up,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 3,
        },
        // Broadside
        Action::Key {
            position: Some(Position {
                x: 47,
                y: 56,
                allow_adjusting: false,
            }),
            key: KeyBinding::W,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Right,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        // Loot
        Action::Move {
            position: Position {
                x: 59,
                y: 56,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 0,
        },
        // Spam
        Action::Key {
            position: None,
            key: KeyBinding::A,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        // Loot
        Action::Move {
            position: Position {
                x: 59,
                y: 41,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 0,
        },
        // Solar Crest
        Action::Key {
            position: None,
            key: KeyBinding::Six,
            condition: ActionCondition::EveryMillis(250000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 10,
        },
        // True Achranid Reflection
        Action::Key {
            position: None,
            key: KeyBinding::F3,
            condition: ActionCondition::EveryMillis(250000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 10,
        },
        Action::Move {
            position: Position {
                x: 40,
                y: 41,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 0,
        },
        // Second Broadside
        Action::Key {
            position: Some(Position {
                x: 47,
                y: 27,
                allow_adjusting: false,
            }),
            key: KeyBinding::W,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Right,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        // Loot
        Action::Move {
            position: Position {
                x: 67,
                y: 13,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 0,
        },
        Action::Key {
            position: None,
            key: KeyBinding::A,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        Action::Move {
            position: Position {
                x: 149,
                y: 13,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 0,
        },
        // Sol Janus
        Action::Key {
            position: Some(Position {
                x: 137,
                y: 33,
                allow_adjusting: false,
            }),
            key: KeyBinding::Four,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 5,
        },
        Action::Key {
            position: None,
            key: KeyBinding::A,
            condition: ActionCondition::ErdaShowerOffCooldown,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        Action::Move {
            position: Position {
                x: 145,
                y: 45,
                allow_adjusting: false,
            },
            condition: ActionCondition::ErdaShowerOffCooldown,
            wait_after_move_ticks: 0,
        },
        // Target Lock
        Action::Key {
            position: None,
            key: KeyBinding::R,
            condition: ActionCondition::EveryMillis(30000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 15,
        },
        // The Dreadnought
        Action::Key {
            position: None,
            key: KeyBinding::F4,
            condition: ActionCondition::EveryMillis(360000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        // Bullet Barrage
        Action::Key {
            position: None,
            key: KeyBinding::D,
            condition: ActionCondition::EveryMillis(90000),
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
        },
        // Spam when erda shower on cooldown
        Action::Key {
            position: Some(Position {
                x: 174,
                y: 58,
                allow_adjusting: false,
            }),
            key: KeyBinding::A,
            condition: ActionCondition::Any,
            direction: ActionKeyDirection::Left,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 5,
        },
    ]
}
