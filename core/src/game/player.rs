use opencv::{
    core::{MatTraitConst, Point},
    prelude::Mat,
};
use platforms::windows::keys::KeyKind;

use super::{
    detector::{detect_player, to_ranges},
    state::{Context, MinimapState, UpdateState},
};

const PLAYER_DETECTION_THRESHOLD: f64 = 0.8;

#[derive(Clone, Copy)]
pub struct Location {
    pub pos: Point,
    pub rect: (Point, Point),
}

pub enum PlayerState {
    Idle(PlayerIdle),
    Moving(PlayerMoving),
    Detecting,
}

impl UpdateState for PlayerState {
    fn update(&self, context: &Context, grayscale: &Mat) -> Self {
        match self {
            PlayerState::Detecting => update_pos(context, grayscale)
                .map(|location| {
                    PlayerState::Idle(PlayerIdle {
                        location,
                        move_to: None,
                    })
                })
                .unwrap_or(PlayerState::Detecting),
            PlayerState::Idle(idle) => {
                if idle.move_to.is_some() {
                    PlayerState::Moving(PlayerMoving {
                        location: idle.location,
                        dest: idle.move_to.unwrap(),
                    })
                } else {
                    match update_pos(context, grayscale) {
                        Some(location) => PlayerState::Idle(PlayerIdle {
                            location,
                            move_to: None,
                        }),
                        None => PlayerState::Detecting,
                    }
                }
            }
            PlayerState::Moving(moving) => {
                context.keys.send(KeyKind::SPACE).unwrap();
                PlayerState::Moving(moving.clone())
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct PlayerIdle {
    pub location: Location,
    move_to: Option<Point>,
}

impl PlayerIdle {
    pub fn move_to(&mut self, to: Point) {
        if self.move_to.is_none() {
            self.move_to = Some(to);
        }
    }
}

#[derive(Clone, Copy)]
pub struct PlayerMoving {
    location: Location,
    dest: Point,
}

impl PlayerMoving {}

fn update_pos(context: &Context, grayscale: &Mat) -> Option<Location> {
    let MinimapState::Idle {
        anchors: _,
        rect: minimap_rect,
    } = &context.minimap
    else {
        return None;
    };
    let vec = to_ranges(minimap_rect).expect("unable to extract minimap rectangle");
    let minimap = grayscale.ranges(&vec).expect("unable to extract minimap");
    let Ok(rect) = detect_player(&minimap, PLAYER_DETECTION_THRESHOLD) else {
        return None;
    };
    let minimap_height = minimap_rect.1.y - minimap_rect.0.y;
    let pos = (rect.0 + rect.1) / 2;
    let pos = Point::new(pos.x, minimap_height - pos.y);
    let rect = (rect.0 + minimap_rect.0, rect.1 + minimap_rect.0);
    if cfg!(debug_assertions) {
        println!("player positon: {:?}", pos)
    }
    Some(Location { pos, rect })
}
