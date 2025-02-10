mod detect;
mod mat;
pub mod minimap;
mod models;
pub mod player;
pub mod skill;

use std::{
    cmp::{self, Reverse},
    collections::{BinaryHeap, VecDeque},
    ops::Not,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use log::debug;
use models::{Action, ActionKind, SkillBinding, UseCondition, UseSite};
use opencv::core::Mat;
use platforms::windows::{
    capture::DynamicCapture,
    handle::Handle,
    keys::{KeyKind, Keys},
};

use mat::OwnedMat;
use minimap::{Minimap, MinimapState};
use player::{Player, PlayerState};
use skill::{Skill, SkillKind, SkillState};

static REQUESTS: LazyLock<Mutex<VecDeque<Request>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

#[derive(Clone, Copy, Debug)]
pub enum Request {
    RotateActions,
}

pub(crate) trait Contextual {
    type Persistent = ();

    fn update(&self, context: &Context, mat: &Mat, state: &mut Self::Persistent) -> Self;
}

#[derive(Debug)]
pub(crate) struct Context {
    pub(crate) keys: Keys,
    pub(crate) minimap: Minimap,
    pub(crate) player: Player,
    pub(crate) skills: Vec<Skill>,
    frame: OwnedMat,
}

pub fn post_request(request: Request) {
    REQUESTS.lock().unwrap().push_back(request);
}

pub fn update_loop() {
    static LOOPING: AtomicBool = AtomicBool::new(false);

    if LOOPING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire)
        .is_ok()
    {
        let handle = Handle::new(Some("MapleStoryClass"), None).unwrap();
        let keys = Keys::new(handle.clone());
        let mut capture = DynamicCapture::new(handle.clone()).unwrap();
        let mut player_state = PlayerState::default();
        let mut minimap_state = MinimapState::default();
        let mut skill_states = [SkillState::new(SkillKind::ErdaShower)];
        let mut actions = populate_actions();
        let mut actions_index = 0;
        let mut actions_backward = false;
        let mut force_cast = false;
        let mut context = Context {
            keys,
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: vec![Skill::Detecting; skill_states.len()],
            frame: OwnedMat::empty(),
        };

        player_state.grappling_key = Some(KeyKind::F);
        player_state.upjump_key = Some(KeyKind::C);

        loop_with_fps(30, || {
            let Ok(mat) = capture.grab().map(OwnedMat::new) else {
                return;
            };
            let (action, index, backward) =
                rotate_actions(&context, &actions, actions_index, actions_backward);
            if let ActionKind::Skill {
                condition: UseCondition::ErdaShowerOffCooldown,
                ..
            } = action.kind
            {
                // TEMP
                if matches!(context.skills[0], Skill::Idle) || force_cast {
                    force_cast = true;
                    if player_state.override_action.is_none() {
                        player_state.override_action = Some(action);
                        actions_index = index;
                        actions_backward = backward;
                    }
                } else if let Skill::Cooldown(_) = context.skills[0] {
                    force_cast = false;
                    actions_index = index;
                    actions_backward = backward;
                }
            } else if player_state.normal_action.is_none() {
                force_cast = false;
                player_state.normal_action = Some(action);
                actions_index = index;
                actions_backward = backward;
            }

            context.minimap = context.minimap.update(&context, &mat, &mut minimap_state);
            context.player = context.player.update(&context, &mat, &mut player_state);
            context.skills = context
                .skills
                .iter()
                .enumerate()
                .map(|(i, skill)| skill.update(&context, &mat, &mut skill_states[i]))
                .collect();
            context.frame = mat;
        });
    }
}

fn populate_actions() -> Vec<Action> {
    let mut actions = Vec::new();
    actions.push(Action {
        x: 149,
        y: 39,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "ErdaShower".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::Y,
            },
            condition: UseCondition::ErdaShowerOffCooldown,
            site: UseSite::AtExact,
        },
    });
    actions.push(Action {
        x: 40,
        y: 39,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "HEXA Broadsite".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::W,
            },
            condition: UseCondition::ErdaShowerOffCooldown,
            site: UseSite::AtExact,
        },
    });
    actions.push(Action {
        x: 108,
        y: 45,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "HEXA Broadsite".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::W,
            },
            condition: UseCondition::ErdaShowerOffCooldown,
            site: UseSite::AtExact,
        },
    });
    // actions.push(Action {
    //     x: 14,
    //     y: 13,
    //     kind: ActionKind::Skill {
    //         skill: models::Skill {
    //             name: "Eight-Legs Easton".to_owned(),
    //             kind: models::SkillKind::Other,
    //             binding: SkillBinding::A,
    //         },
    //         condition: UseCondition::None,
    //         site: UseSite::WithDoubleJump,
    //     },
    // });
    actions.push(Action {
        x: 57,
        y: 13,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "Eight-Legs Easton".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::A,
            },
            condition: UseCondition::None,
            site: UseSite::WithDoubleJump,
        },
    });
    actions.push(Action {
        x: 99,
        y: 13,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "Eight-Legs Easton".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::A,
            },
            condition: UseCondition::None,
            site: UseSite::WithDoubleJump,
        },
    });
    actions.push(Action {
        x: 139,
        y: 13,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "Eight-Legs Easton".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::A,
            },
            condition: UseCondition::None,
            site: UseSite::WithDoubleJump,
        },
    });
    actions.push(Action {
        x: 171,
        y: 13,
        kind: ActionKind::Skill {
            skill: models::Skill {
                name: "Eight-Legs Easton".to_owned(),
                kind: models::SkillKind::Other,
                binding: SkillBinding::A,
            },
            condition: UseCondition::None,
            site: UseSite::WithDoubleJump,
        },
    });
    actions
}

fn rotate_actions(
    context: &Context,
    actions: &[Action],
    index: usize,
    backward: bool,
) -> (Action, usize, bool) {
    let i = if backward {
        actions.len() - index - 1
    } else {
        index
    };
    let item = actions[i].clone();
    let backward = if (index + 1) == actions.len() {
        !backward
    } else {
        backward
    };
    let index = (index + 1) % actions.len();
    (item, index, backward)
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
