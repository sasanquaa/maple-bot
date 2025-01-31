use opencv::{
    core::{MatTraitConst, Point, Rect, Vec4b},
    prelude::Mat,
};

use super::{
    detect::{detect_minimap, detect_minimap_name},
    state::{Context, UpdateState},
};

const MINIMAP_DETECTION_THRESHOLD: f32 = 0.5;
const MINIMAP_ANCHOR_THRESHOLD: u8 = 170;

#[derive(Clone, Copy, Debug)]
pub struct Anchors {
    tl: (Point, Vec4b),
    br: (Point, Vec4b),
}

#[derive(Clone, Copy, Debug)]
pub struct MinimapIdle {
    anchors: Anchors,
    pub bbox: Rect,
}

// #[derive(Debug)]
// pub struct MinimapChanging {
//     anchors: Anchors,
// }

// TODO: implement minimap change
#[derive(Debug)]
pub enum MinimapState {
    Idle(MinimapIdle),
    Detecting,
}

impl UpdateState for MinimapState {
    fn update(&self, context: &Context, mat: &Mat) -> Self {
        match &context.minimap {
            MinimapState::Detecting => {
                let Ok(bbox) = detect_minimap(mat, MINIMAP_DETECTION_THRESHOLD) else {
                    return MinimapState::Detecting;
                };
                let Ok(name) = detect_minimap_name(mat, &bbox) else {
                    return MinimapState::Detecting;
                };
                let size = bbox.width.min(bbox.height) as usize;
                let Some(tl) = anchor_at(mat, bbox.tl(), size, 1) else {
                    return MinimapState::Detecting;
                };
                let Some(br) = anchor_at(mat, bbox.br(), size, -1) else {
                    return MinimapState::Detecting;
                };
                let anchors = Anchors { tl, br };
                if cfg!(debug_assertions) {
                    println!("anchor points: {:?}", anchors);
                }
                MinimapState::Idle(MinimapIdle { anchors, bbox })
            }
            MinimapState::Idle(idle) => {
                let MinimapIdle { anchors, bbox: _ } = idle;
                let tl_pixel = pixel_at(mat, anchors.tl.0);
                let br_pixel = pixel_at(mat, anchors.br.0);
                if tl_pixel != anchors.tl.1 && br_pixel != anchors.br.1 {
                    if cfg!(debug_assertions) {
                        println!(
                            "anchor pixels mismatch: {:?} != {:?}",
                            (tl_pixel, br_pixel),
                            (anchors.tl.1, anchors.br.1)
                        );
                    }
                    return MinimapState::Detecting;
                }
                MinimapState::Idle(*idle)
            }
        }
    }
}

// #[inline(always)]
// fn pixel_to_vec4i(pixel: Vec4b) -> Vec4i {
//     Vec4i::new(
//         pixel[0] as i32,
//         pixel[1] as i32,
//         pixel[2] as i32,
//         pixel[3] as i32,
//     )
// }

fn pixel_at(mat: &Mat, point: Point) -> Vec4b {
    *mat.at_pt::<Vec4b>(point)
        .expect(format!("unable to read pixel at {:?}", point).as_str())
}

fn anchor_at(mat: &Mat, offset: Point, size: usize, sign: i32) -> Option<(Point, Vec4b)> {
    (0..size).find_map(|i| {
        let value = sign * i as i32;
        let diag = offset + Point::new(value, value);
        let pixel = pixel_at(mat, diag);
        if pixel.iter().all(|v| *v >= MINIMAP_ANCHOR_THRESHOLD) {
            Some((diag, pixel))
        } else {
            None
        }
    })
}
