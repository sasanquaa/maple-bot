use opencv::{
    core::{Mat, MatTraitConst, Point, Scalar},
    highgui::{imshow, wait_key},
    imgcodecs::imwrite_def,
    imgproc::{LINE_8, rectangle_points},
};
use platforms::windows::{capture::DynamicCapture, handle::Handle, keys::Keys};

use crate::{
    error::Error,
    game::detector::{detect_player, minimap_bottom_right_template_size, to_ranges},
};

use super::{
    detector::{detect_minimap, minimap_top_left_template_size, to_grayscale},
    mat::OwnedMat,
    player::PlayerState,
};

const MINIMAP_DETECTION_THRESHOLD: f64 = 0.77;
const MINIMAP_ANCHOR_THRESHOLD: u8 = 165;

pub struct Context {
    pub keys: Keys,
    pub minimap: MinimapState,
    pub player: PlayerState,
}

pub trait UpdateState {
    fn update(&self, context: &Context, grayscale: &Mat) -> Self;
}

pub enum MinimapState {
    Idle {
        anchors: ((Point, u8), (Point, u8)),
        rect: (Point, Point),
    },
    Detecting,
    Changing {
        anchors: ((Point, u8), (Point, u8)),
    },
}

fn update_minimap_state(context: &Context, grayscale: &Mat) -> MinimapState {
    fn pixel_at(grayscale: &Mat, point: Point) -> u8 {
        *grayscale
            .at_pt::<u8>(point)
            .expect(format!("unable to read pixel at {:?}", point).as_str())
    }

    fn anchor_at(
        grayscale: &Mat,
        offset: Point,
        size: usize,
        multiplier: i32,
    ) -> Option<(Point, u8)> {
        (0..size)
            .map(|i| {
                let value = multiplier * i as i32;
                let diag = offset + Point::new(value, value);
                let pixel = pixel_at(grayscale, diag);
                (diag, pixel)
            })
            .find(|(_, pixel)| *pixel >= MINIMAP_ANCHOR_THRESHOLD)
    }

    match &context.minimap {
        MinimapState::Detecting => {
            let Ok(rect) = detect_minimap(grayscale, MINIMAP_DETECTION_THRESHOLD) else {
                return MinimapState::Detecting;
            };
            let Some(tl_anchor) = anchor_at(
                grayscale,
                rect.0,
                minimap_top_left_template_size().width as usize,
                1,
            ) else {
                return MinimapState::Detecting;
            };
            let Some(br_anchor) = anchor_at(
                grayscale,
                rect.0,
                minimap_bottom_right_template_size().width as usize,
                1,
            ) else {
                return MinimapState::Detecting;
            };
            let anchors = (tl_anchor, br_anchor);
            if cfg!(debug_assertions) {
                println!("anchor points: {:?}", anchors);
            }
            MinimapState::Idle { anchors, rect }
        }
        MinimapState::Idle { anchors, rect } => {
            let tl_pixel = pixel_at(grayscale, anchors.0.0);
            let br_pixel = pixel_at(grayscale, anchors.1.0);
            if tl_pixel != anchors.0.1 && br_pixel != anchors.1.1 {
                if cfg!(debug_assertions) {
                    println!(
                        "anchor pixels mismatch: {:?} != {:?}",
                        (tl_pixel, br_pixel),
                        (anchors.0.1, anchors.1.1)
                    );
                }
                match detect_minimap(grayscale, MINIMAP_DETECTION_THRESHOLD) {
                    Ok(rect_new) => {
                        // drag
                        if rect_new != *rect {
                            return MinimapState::Detecting;
                        }
                        //  change map
                        let tl_diff = tl_pixel as i32 - anchors.0.1 as i32;
                        let br_diff = br_pixel as i32 - anchors.1.1 as i32;
                        if tl_diff < 0 && br_diff < 0 {
                            return MinimapState::Changing {
                                anchors: anchors.clone(),
                            };
                        }
                    }
                    Err(_) => return MinimapState::Detecting, // UI block
                };
            }
            MinimapState::Idle {
                anchors: anchors.clone(),
                rect: rect.clone(),
            }
        }
        MinimapState::Changing { anchors } => {
            let tl_pixel = pixel_at(grayscale, anchors.0.0);
            let br_pixel = pixel_at(grayscale, anchors.1.0);
            let tl_diff = tl_pixel as i32 - anchors.0.1 as i32;
            let br_diff = br_pixel as i32 - anchors.1.1 as i32;
            if tl_diff <= 0 && br_diff <= 0 {
                if cfg!(debug_assertions) {
                    println!(
                        "minimap changing: {:?} -> {:?}",
                        (anchors.0.1, anchors.1.1),
                        (tl_pixel, br_pixel)
                    );
                }
                MinimapState::Changing {
                    anchors: ((anchors.0.0, tl_pixel), (anchors.1.0, br_pixel)),
                }
            } else {
                if cfg!(debug_assertions) {
                    if cfg!(debug_assertions) {
                        println!(
                            "minimap changed: {:?} -> {:?}",
                            (anchors.0.1, anchors.1.1),
                            (tl_pixel, br_pixel)
                        );
                    }
                }
                MinimapState::Detecting
            }
        }
    }
}

pub fn update_loop() -> Result<(), Error> {
    let handle = Handle::new(Some("MapleStoryClass"), None)?;
    let capture = DynamicCapture::new(handle.clone())?;
    let keys = Keys::new(handle);
    let mut context = Context {
        keys,
        minimap: MinimapState::Detecting,
        player: PlayerState::Detecting,
    };
    loop {
        let Ok(mut mat) = capture.grab().map_err(Error::from).map(OwnedMat::new) else {
            continue;
        };
        let grayscale = to_grayscale(&mat, Some(1.5), Some(-80.0)).unwrap();
        context.minimap = update_minimap_state(&context, &grayscale);
        context.player = context.player.update(&context, &grayscale);
        draw_debug(&mut context, &mut mat);
    }
}

#[cfg(debug_assertions)]
fn draw_debug(context: &mut Context, mat: &mut Mat) {
    let mut mat = to_grayscale(mat, None, None).unwrap();
    match context.minimap {
        MinimapState::Idle { anchors: _, rect } => {
            let _ = rectangle_points(
                &mut mat,
                rect.0,
                rect.1,
                Scalar::from_array([255., 0., 0., 255.]),
                1,
                LINE_8,
                0,
            )
            .unwrap();
        }
        _ => (),
    }
    match &mut context.player {
        PlayerState::Idle(idle) => {
            idle.move_to(Point::new(0, 0));
            let _ = rectangle_points(
                &mut mat,
                idle.location.rect.0,
                idle.location.rect.1,
                Scalar::from_array([255., 0., 0., 255.]),
                1,
                LINE_8,
                0,
            )
            .unwrap();
        }
        _ => (),
    }
    let _ = imshow("Debug", &mat);
    let _ = wait_key(1);
    let _ = imwrite_def((env!("OUT_DIR").to_owned() + "test.png").as_str(), &mat);
}
