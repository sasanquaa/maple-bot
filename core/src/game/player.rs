use std::ops::Range;

use opencv::{
    core::{Point, Rect},
    prelude::Mat,
};
use platforms::windows::keys::KeyKind;

use super::{
    detect::detect_player,
    minimap::MinimapState,
    state::{Context, UpdateState},
};

const PLAYER_DETECTION_THRESHOLD: f64 = 0.4;
const PLAYER_MOVE_THRESHOLD: i32 = 2;
const PLAYER_DOUBLE_JUMP_THRESHOLD: i32 = 20;
const PLAYER_GRAPPLING_THRESHOLD: i32 = 25;
const PLAYER_UP_JUMP_THRESHOLD: i32 = 10;
const PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD: Range<i32> =
    PLAYER_JUMP_THRESHOLD..PLAYER_UP_JUMP_THRESHOLD;
const PLAYER_JUMP_THRESHOLD: i32 = 7;
const PLAYER_MOVEMENT_TIMEOUT: u32 = 3;
const PLAYER_GRAPPLING_TIMEOUT: u32 = 60;
const PLAYER_GRAPPLING_STOPPING_TIMEOUT: u32 = 5;
const PLAYER_UP_JUMP_TIMEOUT: u32 = 7;

#[derive(Clone, Copy, Debug)]
pub struct PlayerIdle {
    pub bbox: Rect,
    pub pos: Point,
    pos_dest: Option<Point>,
}

impl PlayerIdle {
    pub fn move_to(&mut self, dest: Point) {
        if self.pos_dest.is_none() {
            self.pos_dest = Some(dest);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerMoving {
    pos: Point,
    pos_dest: Point,
    timeout: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerGrappling {
    moving: PlayerMoving,
    stopping: bool,
}

#[derive(Debug)]
pub enum PlayerState {
    Idle(PlayerIdle),
    Moving(PlayerMoving),
    HorizontalMoving(PlayerMoving),
    VerticalMoving(PlayerMoving),
    Grappling(PlayerGrappling),
    Jumping(PlayerMoving),
    UpJumping(PlayerMoving),
    Falling(PlayerMoving),
    Detecting,
}

impl UpdateState for PlayerState {
    fn update(&self, context: &Context, mat: &Mat) -> Self {
        let Some((cur_pos, bbox)) = update_pos(context, mat) else {
            return PlayerState::Detecting;
        };
        // TODO: detect if a point is reachable after number of retries?
        // TODO: add unit tests
        match self {
            PlayerState::Detecting => PlayerState::Idle(PlayerIdle {
                bbox,
                pos: cur_pos,
                pos_dest: None,
            }),
            PlayerState::Idle(idle) => idle
                .pos_dest
                .map(|pos_dest| {
                    if cfg!(debug_assertions) {
                        println!("player move to: {:?}", pos_dest);
                    }
                    PlayerState::Moving(PlayerMoving {
                        pos: cur_pos,
                        pos_dest,
                        timeout: 0,
                    })
                })
                .unwrap_or_else(|| PlayerState::Idle(update_idle(idle, cur_pos, bbox))),
            PlayerState::Moving(moving) => {
                let _ = context.keys.send_up(KeyKind::RIGHT);
                let _ = context.keys.send_up(KeyKind::LEFT);
                let _ = context.keys.send_up(KeyKind::DOWN);
                let pos_dest = &moving.pos_dest;
                let (y_distance, _) = y_distance_direction(pos_dest, &cur_pos);
                let (x_distance, _) = x_distance_direction(pos_dest, &cur_pos);
                let moving = update_moving(moving, cur_pos, 0);
                match (x_distance, y_distance) {
                    (x, _) if x >= PLAYER_MOVE_THRESHOLD => PlayerState::HorizontalMoving(moving),
                    // since y is fixed so I think it is okay to check == 0 instead of threshold
                    (_, y) if y != 0 => PlayerState::VerticalMoving(moving),
                    _ => {
                        if cfg!(debug_assertions) {
                            println!(
                                "player reached {:?} with actual pos {:?}",
                                pos_dest, cur_pos
                            );
                        }
                        PlayerState::Idle(PlayerIdle {
                            bbox,
                            pos: cur_pos,
                            pos_dest: None,
                        })
                    }
                }
            }
            PlayerState::HorizontalMoving(moving) => {
                let PlayerMoving {
                    pos: _,
                    pos_dest,
                    timeout,
                } = moving;
                let moving = update_moving_and_timeout(moving, cur_pos, *timeout, true);
                let (x_distance, x_direction) = x_distance_direction(pos_dest, &cur_pos);
                // I really don't know what this mess is but it sure works!
                // x > 0: cur_pos is to the left of pos_dest
                // x < 0: cur_pos is to the right of pos_dest
                match (x_direction, x_distance) {
                    (x, d) if x > 0 && d >= PLAYER_DOUBLE_JUMP_THRESHOLD => {
                        let _ = context.keys.send_up(KeyKind::LEFT);
                        let _ = context.keys.send_down(KeyKind::RIGHT);
                        let _ = context.keys.send(KeyKind::SPACE);
                        let _ = context.keys.send(KeyKind::SPACE);
                    }
                    (x, d) if x < 0 && d >= PLAYER_DOUBLE_JUMP_THRESHOLD => {
                        let _ = context.keys.send_up(KeyKind::RIGHT);
                        let _ = context.keys.send_down(KeyKind::LEFT);
                        let _ = context.keys.send(KeyKind::SPACE);
                        let _ = context.keys.send(KeyKind::SPACE);
                    }
                    (x, d) if x > 0 && d >= PLAYER_MOVE_THRESHOLD => {
                        let _ = context.keys.send_up(KeyKind::LEFT);
                        let _ = context.keys.send_down(KeyKind::RIGHT);
                    }
                    (x, d) if x < 0 && d >= PLAYER_MOVE_THRESHOLD => {
                        let _ = context.keys.send_up(KeyKind::RIGHT);
                        let _ = context.keys.send_down(KeyKind::LEFT);
                    }
                    _ => {
                        let _ = context.keys.send_up(KeyKind::RIGHT);
                        let _ = context.keys.send_up(KeyKind::LEFT);
                    }
                }
                if x_distance < PLAYER_MOVE_THRESHOLD && moving.timeout >= PLAYER_MOVEMENT_TIMEOUT {
                    PlayerState::Moving(moving)
                } else {
                    PlayerState::HorizontalMoving(moving)
                }
            }
            PlayerState::VerticalMoving(moving) => {
                let PlayerMoving {
                    pos: _,
                    pos_dest,
                    timeout,
                } = moving;
                let (x_distance, _) = x_distance_direction(pos_dest, &cur_pos);
                if x_distance > PLAYER_MOVE_THRESHOLD {
                    return PlayerState::Moving(*moving);
                }
                let (y_distance, direction) = y_distance_direction(pos_dest, &cur_pos);
                // y > 0: cur_pos is below pos_dest
                // y < 0: cur_pos is above of pos_dest
                match (direction, y_distance) {
                    (y, d)
                        if y > 0
                            && (d >= PLAYER_GRAPPLING_THRESHOLD
                                || PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD.contains(&d)) =>
                    {
                        let _ = context.keys.send(KeyKind::F);
                        return PlayerState::Grappling(PlayerGrappling {
                            moving: update_moving(&moving, cur_pos, 0),
                            stopping: false,
                        });
                    }
                    (y, d) if y > 0 && d >= PLAYER_UP_JUMP_THRESHOLD => {
                        // TODO: Compound keys up jump
                        // let _ = context.keys.send(KeyKind::C);
                        // return PlayerState::UpJumping(update_moving(&moving, cur_pos, 0));
                        let _ = context.keys.send(KeyKind::F);
                        return PlayerState::Grappling(PlayerGrappling {
                            moving: update_moving(&moving, cur_pos, 0),
                            stopping: false,
                        });
                    }
                    (y, d) if y > 0 && d < PLAYER_JUMP_THRESHOLD => {
                        let _ = context.keys.send(KeyKind::SPACE);
                        return PlayerState::Jumping(update_moving(&moving, cur_pos, 0));
                    }
                    (y, _) if y < 0 => {
                        // this probably won't work if the platforms are far apart,
                        // which is weird to begin with and only happen in very rare place (e.g. Haven)
                        let _ = context.keys.send_down(KeyKind::DOWN);
                        let _ = context.keys.send(KeyKind::SPACE);
                        return PlayerState::Falling(update_moving(&moving, cur_pos, 0));
                    }
                    _ => (),
                }
                let moving = update_moving_and_timeout(moving, cur_pos, *timeout, false);
                if y_distance == 0 && moving.timeout >= PLAYER_MOVEMENT_TIMEOUT {
                    PlayerState::Moving(moving)
                } else {
                    PlayerState::VerticalMoving(moving)
                }
            }
            PlayerState::Grappling(grappling) => {
                let (distance, _) = y_distance_direction(&grappling.moving.pos_dest, &cur_pos);
                if distance <= 2 && !grappling.stopping {
                    let _ = context.keys.send(KeyKind::F);
                    return PlayerState::Grappling(PlayerGrappling {
                        moving: update_moving(&grappling.moving, cur_pos, 0),
                        stopping: true,
                    });
                }
                let timeout_max = if grappling.stopping {
                    PLAYER_GRAPPLING_STOPPING_TIMEOUT
                } else {
                    PLAYER_GRAPPLING_TIMEOUT
                };
                let moving = update_moving_and_timeout(
                    &grappling.moving,
                    cur_pos,
                    grappling.moving.timeout,
                    false,
                );
                if moving.timeout >= timeout_max {
                    return PlayerState::VerticalMoving(PlayerMoving {
                        timeout: 0,
                        ..moving
                    });
                }
                PlayerState::Grappling(PlayerGrappling {
                    moving,
                    ..*grappling
                })
            }
            PlayerState::UpJumping(moving) => update_vertical_state(
                self,
                context,
                moving,
                cur_pos,
                moving.timeout,
                PLAYER_UP_JUMP_TIMEOUT,
            ),
            PlayerState::Jumping(moving) => update_vertical_state(
                self,
                context,
                moving,
                cur_pos,
                moving.timeout,
                PLAYER_MOVEMENT_TIMEOUT,
            ),
            PlayerState::Falling(moving) => update_vertical_state(
                self,
                context,
                moving,
                cur_pos,
                moving.timeout,
                PLAYER_MOVEMENT_TIMEOUT,
            ),
        }
    }
}

#[inline]
fn x_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.x - cur_pos.x;
    let distance = direction.abs();
    (distance, direction)
}

#[inline]
fn y_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.y - cur_pos.y;
    let distance = direction.abs();
    (distance, direction)
}

#[inline]
fn update_moving_and_timeout(
    moving: &PlayerMoving,
    pos: Point,
    timeout: u32,
    horizontal: bool,
) -> PlayerMoving {
    let moved = if horizontal {
        pos.x != moving.pos.x
    } else {
        pos.y != moving.pos.y
    };
    let timeout = if moved { 0 } else { timeout + 1 };
    let moving = update_moving(moving, pos, timeout);
    moving
}

#[inline]
fn update_vertical_state(
    state: &PlayerState,
    context: &Context,
    moving: &PlayerMoving,
    pos: Point,
    timeout: u32,
    timeout_max: u32,
) -> PlayerState {
    let moving = update_moving_and_timeout(moving, pos, timeout, false);
    if moving.timeout >= timeout_max {
        if matches!(state, PlayerState::Falling(_)) {
            let _ = context.keys.send_up(KeyKind::DOWN);
        }
        return PlayerState::VerticalMoving(PlayerMoving {
            timeout: 0,
            ..moving
        });
    }
    match state {
        PlayerState::UpJumping(_) => PlayerState::UpJumping(moving),
        PlayerState::Jumping(_) => PlayerState::Jumping(moving),
        PlayerState::Falling(_) => PlayerState::Falling(moving),
        PlayerState::Idle(_)
        | PlayerState::Grappling(_)
        | PlayerState::Moving(_)
        | PlayerState::HorizontalMoving(_)
        | PlayerState::VerticalMoving(_)
        | PlayerState::Detecting => unreachable!(),
    }
}

#[inline]
fn update_idle(idle: &PlayerIdle, pos: Point, bbox: Rect) -> PlayerIdle {
    PlayerIdle { pos, bbox, ..*idle }
}

#[inline]
fn update_moving(moving: &PlayerMoving, pos: Point, timeout: u32) -> PlayerMoving {
    PlayerMoving {
        pos,
        timeout,
        ..*moving
    }
}

fn update_pos(context: &Context, mat: &Mat) -> Option<(Point, Rect)> {
    let MinimapState::Idle(idle) = &context.minimap else {
        return None;
    };
    let minimap_bbox = idle.bbox;
    let Ok(bbox) = detect_player(mat, &minimap_bbox, PLAYER_DETECTION_THRESHOLD) else {
        return None;
    };
    let tl = bbox.tl() - minimap_bbox.tl();
    let br = bbox.br() - minimap_bbox.tl();
    let pos = (tl + br) / 2;
    let pos = Point::new(pos.x, minimap_bbox.height - pos.y);
    if cfg!(debug_assertions) {
        let prev_pos = match context.player {
            PlayerState::Idle(idle) => idle.pos.into(),
            PlayerState::Moving(moving) => moving.pos.into(),
            PlayerState::HorizontalMoving(moving) => moving.pos.into(),
            PlayerState::VerticalMoving(moving) => moving.pos.into(),
            PlayerState::Grappling(grappling) => grappling.moving.pos.into(),
            PlayerState::Jumping(moving) => moving.pos.into(),
            PlayerState::UpJumping(moving) => moving.pos.into(),
            PlayerState::Falling(moving) => moving.pos.into(),
            PlayerState::Detecting => None,
        };
        if prev_pos.is_none() || prev_pos.unwrap() != pos {
            println!("player pos: {:?} / {:?}", pos, minimap_bbox);
        }
    }
    Some((pos, bbox))
}
