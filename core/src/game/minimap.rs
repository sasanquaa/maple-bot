use opencv::{
    core::{MatTraitConst, Point, Rect},
    prelude::Mat,
};

use super::{
    detector::{
        detect_minimap, minimap_bottom_right_template_size, minimap_top_left_template_size,
    },
    state::{Context, UpdateState},
};

const MINIMAP_DETECTION_THRESHOLD: f64 = 0.77;
const MINIMAP_ANCHOR_THRESHOLD: u8 = 165;

#[derive(Clone, Copy, Debug)]
pub struct Anchors {
    tl: (Point, u8),
    br: (Point, u8),
}

pub struct MinimapIdle {
    pub anchors: Anchors,
    pub rect: Rect,
}

pub struct MinimapChanging {
    anchors: Anchors,
}

pub enum MinimapState {
    Idle(MinimapIdle),
    Detecting,
    Changing(MinimapChanging),
}

impl UpdateState for MinimapState {
    fn update(&self, context: &Context, grayscale: &Mat) -> Self {
        match &context.minimap {
            MinimapState::Detecting => {
                let Ok(rect) = detect_minimap(grayscale, MINIMAP_DETECTION_THRESHOLD) else {
                    return MinimapState::Detecting;
                };
                let Some(tl) = anchor_at(
                    grayscale,
                    rect.tl(),
                    minimap_top_left_template_size().width as usize,
                    1,
                ) else {
                    return MinimapState::Detecting;
                };
                let Some(br) = anchor_at(
                    grayscale,
                    rect.br(),
                    minimap_bottom_right_template_size().width as usize,
                    1,
                ) else {
                    return MinimapState::Detecting;
                };
                let anchors = Anchors { tl, br };
                if cfg!(debug_assertions) {
                    println!("anchor points: {:?}", anchors);
                }
                MinimapState::Idle(MinimapIdle { anchors, rect })
            }
            MinimapState::Idle(MinimapIdle { anchors, rect }) => {
                let tl_pixel = pixel_at(grayscale, anchors.tl.0);
                let br_pixel = pixel_at(grayscale, anchors.br.0);
                if tl_pixel != anchors.tl.1 && br_pixel != anchors.br.1 {
                    if cfg!(debug_assertions) {
                        println!(
                            "anchor pixels mismatch: {:?} != {:?}",
                            (tl_pixel, br_pixel),
                            (anchors.tl.1, anchors.br.1)
                        );
                    }
                    match detect_minimap(grayscale, MINIMAP_DETECTION_THRESHOLD) {
                        Ok(rect_new) => {
                            // drag
                            if rect_new != *rect {
                                return MinimapState::Detecting;
                            }
                            //  change map
                            let tl_diff = tl_pixel as i32 - anchors.tl.1 as i32;
                            let br_diff = br_pixel as i32 - anchors.br.1 as i32;
                            if tl_diff < 0 && br_diff < 0 {
                                return MinimapState::Changing(MinimapChanging {
                                    anchors: anchors.clone(),
                                });
                            }
                        }
                        Err(_) => return MinimapState::Detecting, // UI block
                    };
                }
                MinimapState::Idle(MinimapIdle {
                    anchors: anchors.clone(),
                    rect: rect.clone(),
                })
            }
            MinimapState::Changing(MinimapChanging { anchors }) => {
                let tl_pixel = pixel_at(grayscale, anchors.tl.0);
                let br_pixel = pixel_at(grayscale, anchors.br.0);
                let tl_diff = tl_pixel as i32 - anchors.tl.1 as i32;
                let br_diff = br_pixel as i32 - anchors.br.1 as i32;
                if tl_diff <= 0 && br_diff <= 0 {
                    if cfg!(debug_assertions) {
                        println!(
                            "minimap changing: {:?} -> {:?}",
                            (anchors.tl.1, anchors.br.1),
                            (tl_pixel, br_pixel)
                        );
                    }
                    MinimapState::Changing(MinimapChanging {
                        anchors: Anchors {
                            tl: (anchors.tl.0, tl_pixel),
                            br: (anchors.br.0, br_pixel),
                        },
                    })
                } else {
                    if cfg!(debug_assertions) {
                        if cfg!(debug_assertions) {
                            println!(
                                "minimap changed: {:?} -> {:?}",
                                (anchors.tl.1, anchors.br.1),
                                (tl_pixel, br_pixel)
                            );
                        }
                    }
                    MinimapState::Detecting
                }
            }
        }
    }
}

fn pixel_at(grayscale: &Mat, point: Point) -> u8 {
    *grayscale
        .at_pt::<u8>(point)
        .expect(format!("unable to read pixel at {:?}", point).as_str())
}

fn anchor_at(grayscale: &Mat, offset: Point, size: usize, sign: i32) -> Option<(Point, u8)> {
    (0..size)
        .map(|i| {
            let value = sign * i as i32;
            let diag = offset + Point::new(value, value);
            let pixel = pixel_at(grayscale, diag);
            (diag, pixel)
        })
        .find(|(_, pixel)| *pixel >= MINIMAP_ANCHOR_THRESHOLD)
}
