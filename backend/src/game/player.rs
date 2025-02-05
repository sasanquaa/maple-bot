use std::{ops::Range, thread, time::Duration};

use log::debug;
use opencv::{core::Point, prelude::Mat};
use platforms::windows::keys::KeyKind;

use super::{Context, Contextual, detect::detect_player, minimap::Minimap};

const PLAYER_DETECTION_THRESHOLD: f64 = 0.4;

const PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD: i32 = 1;
const PLAYER_HORIZONTAL_ADJUSTING_LONG_THRESHOLD: i32 = 3;
const PLAYER_VERTICAL_MOVE_THRESHOLD: i32 = 1;
const PLAYER_DOUBLE_JUMP_THRESHOLD: i32 = 12;
const PLAYER_DOUBLE_JUMP_GRAPPLING_THRESHOLD: i32 = PLAYER_DOUBLE_JUMP_THRESHOLD - 5;
const PLAYER_GRAPPLING_THRESHOLD: i32 = 25;
const PLAYER_UP_JUMP_THRESHOLD: i32 = 10;
const PLAYER_JUMP_THRESHOLD: i32 = 7;
const PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD: Range<i32> = const {
    debug_assert!(PLAYER_JUMP_THRESHOLD < PLAYER_UP_JUMP_THRESHOLD);
    PLAYER_JUMP_THRESHOLD..PLAYER_UP_JUMP_THRESHOLD
};
const PLAYER_GRAPPLING_STOPPING_THRESHOLD: i32 = 2;

const PLAYER_MOVE_TIMEOUT: u32 = 4;
const PLAYER_DOUBLE_JUMP_TIMEOUT: u32 = 1;
const PLAYER_GRAPPLING_TIMEOUT: u32 = 60;
const PLAYER_GRAPPLING_STOPPING_TIMEOUT: u32 = 5;
const PLAYER_UP_JUMP_TIMEOUT: u32 = 7;

#[derive(Clone, Copy, Debug)]
pub struct PlayerIdle {
    pos: Point,
    dest: Option<Point>,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerMoving {
    pos: Point,
    dest: Point,
    started: bool,
    timeout: u32,
}

impl PlayerMoving {
    fn pos(self, pos: Point) -> PlayerMoving {
        PlayerMoving { pos, ..self }
    }

    fn started(self, started: bool) -> PlayerMoving {
        PlayerMoving { started, ..self }
    }

    fn timeout(self, timeout: u32) -> PlayerMoving {
        PlayerMoving { timeout, ..self }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerGrappling {
    moving: PlayerMoving,
    stopping: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Player {
    Idle(PlayerIdle),
    Moving(PlayerMoving),
    HorizontalMoving(PlayerMoving),
    HorizontalAdjusting(PlayerMoving),
    DoubleJumping(PlayerMoving),
    VerticalMoving(PlayerMoving),
    Grappling(PlayerGrappling),
    Jumping(PlayerMoving),
    UpJumping(PlayerMoving),
    Falling(PlayerMoving),
    Detecting,
}

impl Player {
    pub fn move_to(&mut self, x: i32, y: i32) {
        if let Player::Idle(idle) = self {
            if idle.dest.is_none() {
                idle.dest = Some(Point::new(x, y));
            }
        }
    }
}

impl Contextual for Player {
    fn update(&self, context: &Context, mat: &Mat, _: ()) -> Self {
        let Some(cur_pos) = update_pos(context, mat) else {
            return Player::Detecting;
        };
        // TODO: detect if a point is reachable after number of retries?
        // TODO: add unit tests
        match self {
            Player::Detecting => Player::Idle(PlayerIdle {
                pos: cur_pos,
                dest: None,
            }),
            Player::Idle(idle) => idle
                .dest
                .map(|dest| {
                    debug!(target: "player", "move to: {dest:?}");
                    Player::Moving(PlayerMoving {
                        pos: cur_pos,
                        dest,
                        started: false,
                        timeout: 0,
                    })
                })
                .unwrap_or_else(|| {
                    Player::Idle(PlayerIdle {
                        pos: cur_pos,
                        ..*idle
                    })
                }),
            Player::Moving(moving) => {
                let moving = moving.pos(cur_pos);
                let (x_distance, _) = x_distance_direction(&moving.dest, &moving.pos);
                let (y_distance, _) = y_distance_direction(&moving.dest, &moving.pos);
                match (x_distance, y_distance) {
                    (x, _) if x >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD => {
                        Player::HorizontalMoving(moving)
                    }
                    (_, y) if y >= PLAYER_VERTICAL_MOVE_THRESHOLD => Player::VerticalMoving(moving),
                    _ => {
                        debug!(
                            target: "player",
                            "reached {:?} with actual position {:?}",
                            moving.dest, moving.pos
                        );
                        Player::Idle(PlayerIdle {
                            pos: cur_pos,
                            dest: None,
                        })
                    }
                }
            }
            Player::HorizontalMoving(moving) => {
                let moving = update_moving_timeout(moving, cur_pos, true).started(false);
                let (x_distance, _) = x_distance_direction(&moving.dest, &moving.pos);
                // x > 0: cur_pos is to the left of dest
                // x < 0: cur_pos is to the right of dest
                match x_distance {
                    d if d >= PLAYER_DOUBLE_JUMP_THRESHOLD => {
                        return Player::DoubleJumping(moving);
                    }
                    d if d >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD => {
                        return Player::HorizontalAdjusting(moving);
                    }
                    _ => (),
                }
                if x_distance < PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD
                    && moving.timeout >= PLAYER_MOVE_TIMEOUT
                {
                    Player::Moving(moving)
                } else {
                    Player::HorizontalMoving(moving)
                }
            }
            Player::HorizontalAdjusting(moving) => {
                let (distance, direction) = x_distance_direction(&moving.dest, &cur_pos);
                update_moving_state(
                    moving,
                    cur_pos,
                    PLAYER_MOVE_TIMEOUT,
                    Some(|| match (distance, direction) {
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
                            let _ = context.keys.send_up(KeyKind::RIGHT);
                            let _ = context.keys.send_down(KeyKind::RIGHT);
                            thread::sleep(Duration::from_millis(15));
                            let _ = context.keys.send_up(KeyKind::RIGHT);
                        }
                        (x, d) if x >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD && d < 0 => {
                            let _ = context.keys.send_up(KeyKind::LEFT);
                            let _ = context.keys.send_up(KeyKind::RIGHT);
                            let _ = context.keys.send_down(KeyKind::LEFT);
                            thread::sleep(Duration::from_millis(15));
                            let _ = context.keys.send_up(KeyKind::LEFT);
                        }
                        _ => (),
                    }),
                    None::<fn()>,
                    |moving| {
                        if distance >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD {
                            Player::HorizontalAdjusting(moving.started(false))
                        } else {
                            let _ = context.keys.send_up(KeyKind::LEFT);
                            let _ = context.keys.send_up(KeyKind::RIGHT);
                            Player::HorizontalMoving(moving.timeout(0))
                        }
                    },
                    true,
                )
            }
            Player::DoubleJumping(moving) => {
                let (distance, direction) = x_distance_direction(&moving.dest, &cur_pos);
                update_moving_state(
                    moving,
                    cur_pos,
                    PLAYER_DOUBLE_JUMP_TIMEOUT,
                    Some(|| {
                        match direction {
                            d if d > 0 => {
                                let _ = context.keys.send_down(KeyKind::RIGHT);
                            }
                            d if d < 0 => {
                                let _ = context.keys.send_down(KeyKind::LEFT);
                            }
                            _ => (),
                        }
                        if distance != 0 {
                            let _ = context.keys.send(KeyKind::SPACE);
                            let _ = context.keys.send(KeyKind::SPACE);
                        }
                    }),
                    Some(|| {
                        let _ = context.keys.send_up(KeyKind::RIGHT);
                        let _ = context.keys.send_up(KeyKind::LEFT);
                    }),
                    |moving| {
                        if distance <= PLAYER_DOUBLE_JUMP_GRAPPLING_THRESHOLD {
                            debug!(target: "player", "performs grappling on double jump");
                            let _ = context.keys.send_up(KeyKind::RIGHT);
                            let _ = context.keys.send_up(KeyKind::LEFT);
                            Player::Grappling(PlayerGrappling {
                                moving: moving.started(false),
                                stopping: false,
                            })
                        } else {
                            Player::DoubleJumping(moving)
                        }
                    },
                    true,
                )
            }
            Player::VerticalMoving(moving) => {
                let moving = update_moving_timeout(moving, cur_pos, false).started(false);
                let (y_distance, direction) = y_distance_direction(&moving.dest, &moving.pos);
                let (x_distance, _) = x_distance_direction(&moving.dest, &moving.pos);
                if x_distance >= PLAYER_HORIZONTAL_ADJUSTING_SHORT_THRESHOLD {
                    return Player::Moving(moving);
                }
                // y > 0: cur_pos is below dest
                // y < 0: cur_pos is above of dest
                match (direction, y_distance) {
                    (y, d)
                        if y > 0
                            && (d >= PLAYER_GRAPPLING_THRESHOLD
                                || PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD.contains(&d)) =>
                    {
                        return Player::Grappling(PlayerGrappling {
                            moving,
                            stopping: false,
                        });
                    }
                    (y, d) if y > 0 && d >= PLAYER_UP_JUMP_THRESHOLD => {
                        // return Player::UpJumping(moving);
                        return Player::Grappling(PlayerGrappling {
                            moving,
                            stopping: false,
                        });
                    }
                    (y, d) if y > 0 && d < PLAYER_JUMP_THRESHOLD => return Player::Jumping(moving),
                    // this probably won't work if the platforms are far apart,
                    // which is weird to begin with and only happen in very rare place (e.g. Haven)
                    (y, _) if y < 0 => return Player::Falling(moving),
                    _ => (),
                }
                if y_distance < PLAYER_VERTICAL_MOVE_THRESHOLD
                    && moving.timeout >= PLAYER_MOVE_TIMEOUT
                {
                    Player::Moving(moving)
                } else {
                    Player::VerticalMoving(moving)
                }
            }
            Player::Grappling(grappling) => {
                let timeout = if grappling.stopping {
                    PLAYER_GRAPPLING_STOPPING_TIMEOUT
                } else {
                    PLAYER_GRAPPLING_TIMEOUT
                };
                update_moving_state(
                    &grappling.moving,
                    cur_pos,
                    timeout,
                    Some(|| {
                        let _ = context.keys.send(KeyKind::F);
                    }),
                    None::<fn()>,
                    |moving| {
                        if !grappling.stopping
                            && y_distance_direction(&moving.dest, &moving.pos).0
                                <= PLAYER_GRAPPLING_STOPPING_THRESHOLD
                        {
                            let _ = context.keys.send(KeyKind::F);
                            Player::Grappling(PlayerGrappling {
                                moving: moving.timeout(0),
                                stopping: true,
                            })
                        } else {
                            Player::Grappling(PlayerGrappling {
                                moving,
                                ..*grappling
                            })
                        }
                    },
                    false,
                )
            }
            Player::UpJumping(moving) => update_moving_state(
                moving,
                cur_pos,
                PLAYER_UP_JUMP_TIMEOUT,
                Some(|| {
                    // why it doesn't work?
                    let _ = context.keys.send_down(KeyKind::UP);
                    let _ = context.keys.send(KeyKind::SPACE);
                    thread::sleep(Duration::from_millis(150));
                    let _ = context.keys.send(KeyKind::SPACE);
                }),
                Some(|| {
                    let _ = context.keys.send_up(KeyKind::UP);
                }),
                Player::UpJumping,
                false,
            ),
            Player::Jumping(moving) => update_moving_state(
                moving,
                cur_pos,
                PLAYER_MOVE_TIMEOUT,
                Some(|| {
                    let _ = context.keys.send(KeyKind::SPACE);
                }),
                None::<fn()>,
                Player::Jumping,
                false,
            ),
            Player::Falling(moving) => update_moving_state(
                moving,
                cur_pos,
                PLAYER_MOVE_TIMEOUT,
                Some(|| {
                    let _ = context.keys.send_down(KeyKind::DOWN);
                    let _ = context.keys.send(KeyKind::SPACE);
                }),
                Some(|| {
                    let _ = context.keys.send_up(KeyKind::DOWN);
                }),
                Player::Falling,
                false,
            ),
        }
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
fn update_moving_timeout(moving: &PlayerMoving, pos: Point, horizontal: bool) -> PlayerMoving {
    let moved = if horizontal {
        pos.x != moving.pos.x
    } else {
        pos.y != moving.pos.y
    };
    let timeout = if moved { 0 } else { moving.timeout + 1 };
    moving.pos(pos).timeout(timeout)
}

#[inline(always)]
fn update_moving_state(
    moving: &PlayerMoving,
    cur_pos: Point,
    timeout: u32,
    on_started: Option<impl FnOnce()>,
    on_timeout: Option<impl FnOnce()>,
    on_state: impl FnOnce(PlayerMoving) -> Player,
    horizontal: bool,
) -> Player {
    match update_moving_timeout(moving, cur_pos, horizontal) {
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
            if horizontal {
                Player::HorizontalMoving(m.timeout(0))
            } else {
                Player::VerticalMoving(m.timeout(0))
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
    let Ok(bbox) = detect_player(mat, &minimap_bbox, PLAYER_DETECTION_THRESHOLD) else {
        return None;
    };
    let tl = bbox.tl() - minimap_bbox.tl();
    let br = bbox.br() - minimap_bbox.tl();
    let pos = Point::new((tl.x + br.x) / 2, minimap_bbox.height - br.y);
    if cfg!(debug_assertions) {
        let prev_pos = match context.player {
            Player::Idle(idle) => idle.pos.into(),
            Player::Moving(moving) => moving.pos.into(),
            Player::HorizontalMoving(moving) => moving.pos.into(),
            Player::DoubleJumping(moving) => moving.pos.into(),
            Player::VerticalMoving(moving) => moving.pos.into(),
            Player::Grappling(grappling) => grappling.moving.pos.into(),
            Player::HorizontalAdjusting(moving) => moving.pos.into(),
            Player::Jumping(moving) => moving.pos.into(),
            Player::UpJumping(moving) => moving.pos.into(),
            Player::Falling(moving) => moving.pos.into(),
            Player::Detecting => None,
        };
        if prev_pos.is_none() || prev_pos.unwrap() != pos {
            debug!(target: "player", "position updated in minimap: {:?} in {:?}", pos, minimap_bbox);
        }
    }
    Some(pos)
}
