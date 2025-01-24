use std::{thread, time::Duration};

use opencv::{
    core::{MatTraitConst, Point, Rect, abs},
    prelude::Mat,
};
use platforms::windows::keys::KeyKind;

use super::{
    detector::{detect_player, to_ranges},
    minimap::MinimapState,
    state::{Context, UpdateState},
};

const PLAYER_DETECTION_THRESHOLD: f64 = 0.8;
const PLAYER_MOVE_THRESHOLD: i32 = 3;
const PLAYER_DOUBLE_JUMP_THRESHOLD: i32 = 15;

#[derive(Clone, Copy)]
pub struct PlayerIdle {
    pub rect: Rect,
    pos: Point,
    pos_dest: Option<Point>,
}

impl PlayerIdle {
    pub fn move_to(&mut self, dest: Point) {
        if self.pos_dest.is_none() {
            self.pos_dest = Some(dest);
        }
    }
}

#[derive(Clone, Copy)]
pub struct PlayerMoving {
    pos: Point,
    pos_dest: Point,
}

impl PlayerMoving {}

pub enum PlayerState {
    Idle(PlayerIdle),
    Moving(PlayerMoving),
    Detecting,
}

impl UpdateState for PlayerState {
    fn update(&self, context: &Context, grayscale: &Mat) -> Self {
        match self {
            PlayerState::Detecting => update_pos(context, grayscale)
                .map(|(pos, rect)| {
                    PlayerState::Idle(PlayerIdle {
                        pos,
                        rect,
                        pos_dest: None,
                    })
                })
                .unwrap_or(PlayerState::Detecting),
            PlayerState::Idle(idle) => {
                if idle.pos_dest.is_some() {
                    PlayerState::Moving(PlayerMoving {
                        pos: idle.pos,
                        pos_dest: idle.pos_dest.unwrap(),
                    })
                } else {
                    match update_pos(context, grayscale) {
                        Some((pos, rect)) => PlayerState::Idle(PlayerIdle {
                            rect,
                            pos,
                            pos_dest: None,
                        }),
                        None => PlayerState::Detecting,
                    }
                }
            }
            PlayerState::Moving(PlayerMoving { pos: _, pos_dest }) => {
                let (cur_pos, _) = update_pos(context, grayscale).unwrap();
                let dir = *pos_dest - cur_pos;
                let dist = Point::new(dir.x.abs(), dir.y.abs());
                if dir.x > 0 && dist.x > PLAYER_MOVE_THRESHOLD {
                    // pos is to the right of pos_dest
                    if dist.x >= PLAYER_DOUBLE_JUMP_THRESHOLD {
                        let _ = context.keys.send_key_up(KeyKind::LEFT);
                        let _ = context.keys.send_key_down(KeyKind::RIGHT);
                        let _ = context.keys.send(KeyKind::SPACE);
                        let _ = context.keys.send(KeyKind::SPACE);
                    }
                } else if dir.x < 0 && dist.x > PLAYER_MOVE_THRESHOLD {
                    if dist.x >= PLAYER_DOUBLE_JUMP_THRESHOLD {
                        let _ = context.keys.send_key_up(KeyKind::RIGHT);
                        let _ = context.keys.send_key_down(KeyKind::LEFT);
                        let _ = context.keys.send(KeyKind::SPACE);
                        let _ = context.keys.send(KeyKind::SPACE);
                    }
                } else {
                    let _ = context.keys.send_key_up(KeyKind::LEFT);
                    let _ = context.keys.send_key_up(KeyKind::RIGHT);
                }
                if dir.y > 0 && dist.x <= PLAYER_MOVE_THRESHOLD {
                    // pos is below pos_dest
                    // TODO: GRAPLING HOOK?
                    if dist.y >= PLAYER_MOVE_THRESHOLD {
                        let _ = context.keys.send(KeyKind::F);
                    }
                }
                println!("{:?} / {:?}", dist, dir);
                PlayerState::Moving(PlayerMoving {
                    pos: cur_pos,
                    pos_dest: *pos_dest,
                })
            }
        }
    }
}

fn update_pos(context: &Context, grayscale: &Mat) -> Option<(Point, Rect)> {
    let MinimapState::Idle(idle) = &context.minimap else {
        return None;
    };
    let minimap_rect = idle.rect;
    let vec = to_ranges(&minimap_rect).expect("unable to extract minimap rectangle");
    let minimap = grayscale.ranges(&vec).expect("unable to extract minimap");
    let Ok(rect) = detect_player(&minimap, PLAYER_DETECTION_THRESHOLD) else {
        return None;
    };
    let pos = (rect.tl() + rect.br()) / 2;
    let pos = Point::new(pos.x, minimap_rect.height - pos.y);
    let rect = Rect::from_points(rect.tl() + minimap_rect.tl(), rect.br() + minimap_rect.br());
    if cfg!(debug_assertions) {
        println!("player positon: {:?}", pos)
    }
    Some((pos, rect))
}
