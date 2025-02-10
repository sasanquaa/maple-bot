use std::ops::Range;

use log::debug;
use opencv::{core::Point, prelude::Mat};
use platforms::windows::keys::KeyKind;

use crate::game::models::UseSite;

use super::{
    Context, Contextual,
    detect::detect_player,
    minimap::Minimap,
    models::{Action, ActionKind, SkillBinding, SkillKind},
};

const PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD: i32 = 1;
const PLAYER_HORIZONTAL_ADJUSTING_LONG_THRESHOLD: i32 = 3;
const PLAYER_VERTICAL_MOVE_THRESHOLD: i32 = 1;
const PLAYER_DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Maximum amount of ticks a change in x or y direction must be detected
const PLAYER_MOVE_TIMEOUT: u32 = 4;

#[derive(Debug, Default)]
pub struct PlayerState {
    pub normal_action: Option<Action>,
    pub override_action: Option<Action>,
    pub grappling_key: Option<KeyKind>,
    pub upjump_key: Option<KeyKind>,
}

#[derive(Clone, Copy)]
enum ChangeAxis {
    Horizontal,
    Vertical,
    Both(fn(Point) -> Player),
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerMoving {
    pos: Point,
    dest: Point,
    started: bool,
    completed: bool,
    timeout: u32,
}

impl PlayerMoving {
    fn new(pos: Point, dest: Point) -> Self {
        Self {
            pos,
            dest,
            started: false,
            completed: false,
            timeout: 0,
        }
    }

    fn pos(self, pos: Point) -> PlayerMoving {
        PlayerMoving { pos, ..self }
    }

    fn started(self, started: bool) -> PlayerMoving {
        PlayerMoving { started, ..self }
    }

    fn completed(self, completed: bool) -> PlayerMoving {
        PlayerMoving { completed, ..self }
    }

    fn timeout(self, timeout: u32) -> PlayerMoving {
        PlayerMoving { timeout, ..self }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerDoubleJumping {
    moving: PlayerMoving,
    force_double_jump: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Player {
    Detecting,
    Idle(Point),
    UseSkill,
    Moving(Point),
    HorizontalMoving(Point),
    HorizontalAdjusting(PlayerMoving),
    DoubleJumping(PlayerDoubleJumping),
    VerticalMoving(Point),
    Grappling(PlayerMoving),
    Jumping(PlayerMoving),
    UpJumping(PlayerMoving),
    Falling(PlayerMoving),
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // 草草ｗｗ。。。
    fn update(&self, context: &Context, mat: &Mat, state: &mut PlayerState) -> Self {
        let Some(cur_pos) = update_pos(context, mat) else {
            return Player::Detecting;
        };
        // TODO: detect if a point is reachable after number of retries?
        // TODO: add unit tests
        update(self, context, cur_pos, state)
    }
}

fn update(current: &Player, context: &Context, cur_pos: Point, state: &mut PlayerState) -> Player {
    // params order context -> state -> cur_pos -> dest -> moving
    match current {
        Player::Detecting => Player::Idle(cur_pos),
        Player::Idle(_) => update_idle(state, cur_pos),
        Player::UseSkill => on_request(
            state,
            |action| {
                if let ActionKind::Skill { skill, .. } = &action.kind {
                    let moving = PlayerMoving::new(cur_pos, cur_pos);
                    let terminal = match skill.kind {
                        SkillKind::RopeLift => Player::Grappling(moving),
                        SkillKind::UpJump => Player::UpJumping(moving),
                        SkillKind::DoubleJump => Player::DoubleJumping(PlayerDoubleJumping {
                            moving,
                            force_double_jump: true,
                        }),
                        SkillKind::Other => {
                            let key = match skill.binding {
                                SkillBinding::Y => KeyKind::Y,
                                SkillBinding::F => KeyKind::F,
                                SkillBinding::C => KeyKind::C,
                                SkillBinding::A => KeyKind::A,
                                SkillBinding::W => KeyKind::W,
                            };
                            let _ = context.keys.send(key);
                            Player::Idle(cur_pos)
                        }
                    };
                    return Some((terminal, true));
                }
                Some((Player::Idle(cur_pos), true))
            },
            || Player::Idle(cur_pos),
        ),
        Player::Moving(dest) => update_moving(state, cur_pos, *dest),
        Player::HorizontalMoving(dest) => update_horizontal_moving(cur_pos, *dest),
        Player::HorizontalAdjusting(moving) => {
            update_horizontal_adjusting(context, state, cur_pos, moving)
        }
        Player::DoubleJumping(jumping) => update_double_jumping(context, state, cur_pos, jumping),
        Player::VerticalMoving(dest) => update_vertical_moving(cur_pos, *dest),
        Player::Grappling(moving) => update_grappling(context, state, cur_pos, moving),
        Player::UpJumping(moving) => update_up_jumping(context, state, cur_pos, moving),
        Player::Jumping(moving) => update_moving_state(
            moving,
            cur_pos,
            PLAYER_MOVE_TIMEOUT,
            Some(|| {
                let _ = context.keys.send(KeyKind::SPACE);
            }),
            None::<fn()>,
            |moving| {
                on_request(
                    state,
                    |action| {
                        if let ActionKind::Jump = action.kind {
                            return Some((Player::Idle(cur_pos), true));
                        }
                        None
                    },
                    || Player::Jumping(moving),
                )
            },
            ChangeAxis::Vertical,
        ),
        Player::Falling(moving) => {
            let y_changed = (cur_pos.y - moving.pos.y).abs();
            update_moving_state(
                moving,
                cur_pos,
                PLAYER_MOVE_TIMEOUT,
                Some(|| {
                    let _ = context.keys.send_down(KeyKind::DOWN);
                }),
                Some(|| {
                    let _ = context.keys.send_up(KeyKind::DOWN);
                }),
                |mut moving| {
                    if !moving.completed {
                        if y_changed == 0 {
                            let _ = context.keys.send(KeyKind::SPACE);
                        } else {
                            moving.completed = true;
                        }
                    }
                    Player::Falling(moving)
                },
                ChangeAxis::Vertical,
            )
        }
    }
}

fn update_idle(state: &mut PlayerState, cur_pos: Point) -> Player {
    on_request(
        state,
        |action| match action.kind {
            ActionKind::Wait(_) => todo!(),
            ActionKind::Jump => Some((Player::Jumping(PlayerMoving::new(cur_pos, cur_pos)), false)),
            ActionKind::Move { .. } | ActionKind::Skill { .. } => {
                debug!(target: "player", "handling move (and maybe attack) at: {} {}", action.x, action.y);
                Some((Player::Moving(Point::new(action.x, action.y)), false))
            }
        },
        || Player::Idle(cur_pos),
    )
}

fn update_moving(state: &mut PlayerState, cur_pos: Point, dest: Point) -> Player {
    let (x_distance, _) = x_distance_direction(&dest, &cur_pos);
    let (y_distance, _) = y_distance_direction(&dest, &cur_pos);
    match (x_distance, y_distance) {
        (x, _) if x >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD => {
            Player::HorizontalMoving(dest)
        }
        (_, y) if y >= PLAYER_VERTICAL_MOVE_THRESHOLD => Player::VerticalMoving(dest),
        _ => {
            debug!(
                target: "player",
                "reached {:?} with actual position {:?}",
                dest, cur_pos
            );
            on_request(
                state,
                |action| {
                    if let ActionKind::Skill { site, .. } = action.kind {
                        return match site {
                            UseSite::WithDoubleJump => Some((
                                Player::DoubleJumping(PlayerDoubleJumping {
                                    moving: PlayerMoving::new(cur_pos, dest),
                                    force_double_jump: true,
                                }),
                                false,
                            )),
                            UseSite::AtProximity | UseSite::AtExact => {
                                Some((Player::UseSkill, false))
                            }
                        };
                    }
                    None
                },
                || Player::Idle(cur_pos),
            )
        }
    }
}

fn update_horizontal_moving(cur_pos: Point, dest: Point) -> Player {
    let moving = PlayerMoving::new(cur_pos, dest);
    let (x_distance, _) = x_distance_direction(&moving.dest, &moving.pos);
    // x > 0: cur_pos is to the left of dest
    // x < 0: cur_pos is to the right of dest
    match x_distance {
        d if d >= PLAYER_DOUBLE_JUMP_THRESHOLD => Player::DoubleJumping(PlayerDoubleJumping {
            moving,
            force_double_jump: false,
        }),
        d if d >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD => {
            Player::HorizontalAdjusting(moving)
        }
        _ => {
            if x_distance < PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD {
                Player::Moving(moving.dest)
            } else {
                Player::HorizontalMoving(moving.dest)
            }
        }
    }
}

fn update_horizontal_adjusting(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: &PlayerMoving,
) -> Player {
    const USE_SKILL_AT_X_PROXIMITY_THRESHOLD: i32 = 12;
    const USE_SKILL_AT_Y_PROXIMITY_THRESHOLD: i32 = 4;

    update_moving_state(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT,
        None::<fn()>,
        None::<fn()>,
        |moving| {
            let (x_distance, x_direction) = x_distance_direction(&moving.dest, &moving.pos);
            let (y_distance, _) = y_distance_direction(&moving.dest, &moving.pos);
            match (x_distance, x_direction) {
                (x, d) if x >= PLAYER_HORIZONTAL_ADJUSTING_LONG_THRESHOLD && d > 0 => {
                    let _ = context.keys.send_up(KeyKind::LEFT);
                    let _ = context.keys.send_down(KeyKind::RIGHT);
                }
                (x, d) if x >= PLAYER_HORIZONTAL_ADJUSTING_LONG_THRESHOLD && d < 0 => {
                    let _ = context.keys.send_up(KeyKind::RIGHT);
                    let _ = context.keys.send_down(KeyKind::LEFT);
                }
                (x, d) if x >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD && d > 0 => {
                    let _ = context.keys.send_up(KeyKind::LEFT);
                    let _ = context.keys.send_down(KeyKind::RIGHT);
                    if moving.timeout >= 2 {
                        let _ = context.keys.send_up(KeyKind::RIGHT);
                    }
                }
                (x, d) if x >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD && d < 0 => {
                    let _ = context.keys.send_up(KeyKind::RIGHT);
                    let _ = context.keys.send_down(KeyKind::LEFT);
                    if moving.timeout >= 2 {
                        let _ = context.keys.send_up(KeyKind::LEFT);
                    }
                }
                _ => (),
            }
            on_request(
                state,
                |action| {
                    if let ActionKind::Skill { site, .. } = action.kind {
                        match site {
                            UseSite::WithDoubleJump => {
                                let _ = context.keys.send_up(KeyKind::RIGHT);
                                let _ = context.keys.send_up(KeyKind::LEFT);
                                return Some((
                                    Player::DoubleJumping(PlayerDoubleJumping {
                                        moving: PlayerMoving::new(cur_pos, moving.dest),
                                        force_double_jump: true,
                                    }),
                                    false,
                                ));
                            }
                            UseSite::AtProximity => {
                                let _ = context.keys.send_up(KeyKind::RIGHT);
                                let _ = context.keys.send_up(KeyKind::LEFT);
                                if x_distance <= USE_SKILL_AT_X_PROXIMITY_THRESHOLD
                                    && y_distance <= USE_SKILL_AT_Y_PROXIMITY_THRESHOLD
                                {
                                    return Some((Player::UseSkill, false));
                                }
                            }
                            UseSite::AtExact => (),
                        };
                    }
                    None
                },
                || {
                    if x_distance >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD {
                        Player::HorizontalAdjusting(moving)
                    } else {
                        let _ = context.keys.send_up(KeyKind::LEFT);
                        let _ = context.keys.send_up(KeyKind::RIGHT);
                        Player::HorizontalMoving(moving.dest)
                    }
                },
            )
        },
        ChangeAxis::Horizontal,
    )
}

fn update_double_jumping(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    jumping: &PlayerDoubleJumping,
) -> Player {
    const DOUBLE_JUMP_USE_SKILL_X_PROXIMITY_THRESHOLD: i32 = 50;
    const DOUBLE_JUMP_USE_SKILL_Y_PROXIMITY_THRESHOLD: i32 = 4;
    const DOUBLE_JUMP_GRAPPLING_THRESHOLD: i32 = 4;
    const DOUBLE_JUMPED_FORCE_THRESHOLD: i32 = 3;

    let PlayerDoubleJumping {
        moving,
        force_double_jump,
    } = jumping;
    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = x_distance_direction(&moving.dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&moving.dest, &cur_pos);
    update_moving_state(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT,
        Some(|| match x_direction {
            d if d > 0 => {
                let _ = context.keys.send_down(KeyKind::RIGHT);
            }
            d if d < 0 => {
                let _ = context.keys.send_down(KeyKind::LEFT);
            }
            _ => (),
        }),
        None::<fn()>,
        |mut moving| {
            if !moving.completed {
                if (!force_double_jump && x_distance >= PLAYER_DOUBLE_JUMP_THRESHOLD)
                    || (*force_double_jump && x_changed <= DOUBLE_JUMPED_FORCE_THRESHOLD)
                {
                    let _ = context.keys.send(KeyKind::SPACE);
                } else {
                    let _ = context.keys.send_up(KeyKind::RIGHT);
                    let _ = context.keys.send_up(KeyKind::LEFT);
                    moving = moving.completed(true);
                }
            }
            on_request(
                state,
                |action| {
                    if let ActionKind::Skill {
                        site: UseSite::WithDoubleJump | UseSite::AtProximity,
                        ..
                    } = action.kind
                    {
                        if moving.completed && *force_double_jump {
                            // FIXME: maybe a different way to handle this?
                            // The idea is force_double_jump is only used
                            // when the player has already reached or very close to the destination
                            // which is either adjusting state or terminal state of moving
                            // but at least one double jump must be performed and then use the skill
                            let _ = context.keys.send(KeyKind::A);
                            return Some((Player::Idle(cur_pos), true));
                            // return Some((Player::UseSkill, false));
                        } else if moving.completed
                            && x_distance <= DOUBLE_JUMP_USE_SKILL_X_PROXIMITY_THRESHOLD
                            && y_distance <= DOUBLE_JUMP_USE_SKILL_Y_PROXIMITY_THRESHOLD
                            && !force_double_jump
                        {
                            return Some((
                                Player::DoubleJumping(PlayerDoubleJumping {
                                    moving: moving.completed(false).started(false),
                                    force_double_jump: true,
                                }),
                                false,
                            ));
                        }
                    }
                    None
                },
                || {
                    if moving.completed
                        && !force_double_jump
                        && x_distance <= DOUBLE_JUMP_GRAPPLING_THRESHOLD
                        && y_direction > 0
                    {
                        debug!(target: "player", "performs grappling on double jump");
                        Player::Grappling(moving.started(false).completed(false))
                    } else {
                        Player::DoubleJumping(PlayerDoubleJumping { moving, ..*jumping })
                    }
                },
            )
        },
        ChangeAxis::Both(Player::HorizontalMoving),
    )
}

fn update_vertical_moving(cur_pos: Point, dest: Point) -> Player {
    const PLAYER_GRAPPLING_THRESHOLD: i32 = 25;
    const PLAYER_UP_JUMP_THRESHOLD: i32 = 10;
    const PLAYER_JUMP_THRESHOLD: i32 = 7;
    const PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD: Range<i32> = const {
        debug_assert!(PLAYER_JUMP_THRESHOLD < PLAYER_UP_JUMP_THRESHOLD);
        PLAYER_JUMP_THRESHOLD..PLAYER_UP_JUMP_THRESHOLD
    };

    let moving = PlayerMoving::new(cur_pos, dest);
    let (y_distance, direction) = y_distance_direction(&moving.dest, &moving.pos);
    let (x_distance, _) = x_distance_direction(&moving.dest, &moving.pos);
    if x_distance >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD {
        return Player::Moving(moving.dest);
    }
    // y > 0: cur_pos is below dest
    // y < 0: cur_pos is above of dest
    match (direction, y_distance) {
        (y, d)
            if y > 0
                && (d >= PLAYER_GRAPPLING_THRESHOLD
                    || PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD.contains(&d)) =>
        {
            Player::Grappling(moving)
        }
        (y, d) if y > 0 && d >= PLAYER_UP_JUMP_THRESHOLD => Player::UpJumping(moving),
        (y, d) if y > 0 && d < PLAYER_JUMP_THRESHOLD => Player::Jumping(moving),
        // this probably won't work if the platforms are far apart,
        // which is weird to begin with and only happen in very rare place (e.g. Haven)
        (y, _) if y < 0 => Player::Falling(moving),
        _ => {
            if y_distance < PLAYER_VERTICAL_MOVE_THRESHOLD {
                Player::Moving(moving.dest)
            } else {
                Player::VerticalMoving(moving.dest)
            }
        }
    }
}

fn update_grappling(
    context: &Context,
    state: &PlayerState,
    cur_pos: Point,
    moving: &PlayerMoving,
) -> Player {
    const PLAYER_GRAPPLING_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 15;
    const PLAYER_GRAPPLING_STOPPING_THRESHOLD: i32 = 2;

    if state.grappling_key.is_none() {
        debug!(target: "player", "failed to use grappling as key is not set");
        return Player::Idle(cur_pos);
    }

    let x_changed = cur_pos.x != moving.pos.x;
    update_moving_state(
        moving,
        cur_pos,
        PLAYER_GRAPPLING_TIMEOUT,
        Some(|| {
            let _ = context.keys.send(state.grappling_key.unwrap());
        }),
        None::<fn()>,
        |mut moving| {
            let (distance, _) = y_distance_direction(&moving.dest, &moving.pos);
            if moving.timeout > 0 && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout(PLAYER_GRAPPLING_TIMEOUT);
            } else if !moving.completed {
                if distance <= PLAYER_GRAPPLING_STOPPING_THRESHOLD {
                    let _ = context.keys.send(state.grappling_key.unwrap());
                } else {
                    moving = moving.completed(true);
                }
            } else if distance == 0 && moving.timeout >= PLAYER_MOVE_TIMEOUT {
                moving = moving.timeout(PLAYER_GRAPPLING_TIMEOUT);
            }
            Player::Grappling(moving)
        },
        ChangeAxis::Vertical,
    )
}

fn update_up_jumping(
    context: &Context,
    state: &PlayerState,
    cur_pos: Point,
    moving: &PlayerMoving,
) -> Player {
    const UP_JUMPED_THRESHOLD: i32 = 4;

    let y_changed = (cur_pos.y - moving.pos.y).abs();
    update_moving_state(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT,
        Some(|| {
            let _ = context.keys.send_down(KeyKind::UP);
        }),
        Some(|| {
            let _ = context.keys.send_up(KeyKind::UP);
        }),
        |mut moving| {
            if !moving.completed {
                if state.upjump_key.is_some() {
                    let _ = context.keys.send(state.upjump_key.unwrap());
                    moving = moving.completed(true);
                } else if y_changed <= UP_JUMPED_THRESHOLD {
                    // spamming space until the player y changes
                    // above a threshold as sending space twice
                    // doesn't work
                    let _ = context.keys.send(KeyKind::SPACE);
                } else {
                    moving = moving.completed(true);
                }
            }
            Player::UpJumping(moving)
        },
        ChangeAxis::Vertical,
    )
}

#[inline(always)]
fn on_request(
    state: &mut PlayerState,
    on_next: impl FnOnce(&Action) -> Option<(Player, bool)>,
    on_default: impl FnOnce() -> Player,
) -> Player {
    if let Some(action) = state
        .override_action
        .as_ref()
        .or(state.normal_action.as_ref())
    {
        let Some((next, is_terminal)) = on_next(action) else {
            return on_default();
        };
        if is_terminal {
            if state.override_action.is_some() {
                state.override_action.take();
            } else {
                state.normal_action.take();
            }
        }
        next
    } else {
        on_default()
    }
}

#[inline(always)]
fn x_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.x - cur_pos.x;
    let distance = direction.abs();
    (distance, direction)
}

#[inline(always)]
fn y_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.y - cur_pos.y;
    let distance = direction.abs();
    (distance, direction)
}

#[inline(always)]
fn update_moving_timeout(
    moving: &PlayerMoving,
    cur_pos: Point,
    timeout: u32,
    axis: ChangeAxis,
) -> PlayerMoving {
    if moving.timeout >= timeout {
        return moving.pos(cur_pos);
    }
    let moved = match axis {
        ChangeAxis::Horizontal => cur_pos.x != moving.pos.x,
        ChangeAxis::Vertical => cur_pos.y != moving.pos.y,
        ChangeAxis::Both { .. } => cur_pos.x != moving.pos.x || cur_pos.y != moving.pos.y,
    };
    let timeout = if moved { 0 } else { moving.timeout + 1 };
    moving.pos(cur_pos).timeout(timeout)
}

#[inline(always)]
fn update_moving_state(
    moving: &PlayerMoving,
    cur_pos: Point,
    timeout: u32,
    on_started: Option<impl FnOnce()>,
    on_timeout: Option<impl FnOnce()>,
    on_state: impl FnOnce(PlayerMoving) -> Player,
    axis: ChangeAxis,
) -> Player {
    match update_moving_timeout(moving, cur_pos, timeout, axis) {
        m if !m.started => {
            if let Some(callback) = on_started {
                callback();
            }
            on_state(m.timeout(0).started(true))
        }
        m if m.timeout >= timeout => {
            if let Some(callback) = on_timeout {
                callback();
            }
            match axis {
                ChangeAxis::Horizontal => Player::HorizontalMoving(m.dest),
                ChangeAxis::Vertical => Player::VerticalMoving(m.dest),
                ChangeAxis::Both(terminal) => terminal(m.dest),
            }
        }
        m => on_state(m),
    }
}

#[inline(always)]
fn update_pos(context: &Context, mat: &Mat) -> Option<Point> {
    let Minimap::Idle(idle) = &context.minimap else {
        return None;
    };
    let minimap_bbox = idle.bbox;
    let Ok(bbox) = detect_player(mat, &minimap_bbox) else {
        return None;
    };
    let tl = bbox.tl() - minimap_bbox.tl();
    let br = bbox.br() - minimap_bbox.tl();
    let x = ((tl.x + br.x) / 2) as f32 / idle.scale_w;
    let y = (minimap_bbox.height - br.y) as f32 / idle.scale_h;
    let pos = Point::new(x as i32, y as i32);
    if cfg!(debug_assertions) {
        let prev_pos = match context.player {
            Player::DoubleJumping(jumping) => jumping.moving.pos.into(),
            Player::Grappling(moving) => moving.pos.into(),
            Player::HorizontalAdjusting(moving) => moving.pos.into(),
            Player::Jumping(moving) => moving.pos.into(),
            Player::UpJumping(moving) => moving.pos.into(),
            Player::Falling(moving) => moving.pos.into(),
            Player::Idle(pos) => pos.into(),
            Player::Moving(_)
            | Player::HorizontalMoving(_)
            | Player::VerticalMoving(_)
            | Player::UseSkill
            | Player::Detecting => None,
        };
        if prev_pos.is_none() || prev_pos.unwrap() != pos {
            // debug!(target: "player", "position updated in minimap: {:?} in {:?}", pos, minimap_bbox);
        }
    }
    Some(pos)
}
