use log::debug;
use opencv::{
    core::{MatTraitConst, Point, Rect, Vec4b},
    prelude::Mat,
};

use super::{
    Context, Contextual,
    detect::{detect_minimap, detect_minimap_name},
};

#[derive(Clone, Copy, Debug)]
struct Anchors {
    tl: (Point, Vec4b),
    br: (Point, Vec4b),
}

#[derive(Clone, Copy, Debug)]
pub struct MinimapIdle {
    anchors: Anchors,
    pub(crate) bbox: Rect,
    pub(crate) bbox_name: Rect,
}

#[derive(Clone, Copy, Debug)]
pub enum Minimap {
    Idle(MinimapIdle),
    Detecting,
}

impl Contextual for Minimap {
    fn update(&self, context: &Context, mat: &Mat, _: ()) -> Self {
        match &context.minimap {
            Minimap::Detecting => {
                let Ok(bbox) = detect_minimap(mat, 0.5) else {
                    return Minimap::Detecting;
                };
                let Ok(bbox_name) = detect_minimap_name(mat, &bbox, 0.7) else {
                    return Minimap::Detecting;
                };
                let size = bbox.width.min(bbox.height) as usize;
                let Some(tl) = anchor_at(mat, bbox.tl(), size, 1) else {
                    return Minimap::Detecting;
                };
                let Some(br) = anchor_at(mat, bbox.br(), size, -1) else {
                    return Minimap::Detecting;
                };
                let anchors = Anchors { tl, br };
                debug!(target: "minimap", "anchor points: {:?}", anchors);
                Minimap::Idle(MinimapIdle {
                    anchors,
                    bbox,
                    bbox_name,
                })
            }
            Minimap::Idle(idle) => {
                let MinimapIdle {
                    anchors,
                    bbox: _,
                    bbox_name: _,
                } = idle;
                let tl_pixel = pixel_at(mat, anchors.tl.0);
                let br_pixel = pixel_at(mat, anchors.br.0);
                if tl_pixel != anchors.tl.1 && br_pixel != anchors.br.1 {
                    debug!(
                        target: "minimap",
                        "anchor pixels mismatch: {:?} != {:?}",
                        (tl_pixel, br_pixel),
                        (anchors.tl.1, anchors.br.1)
                    );
                    return Minimap::Detecting;
                }
                Minimap::Idle(*idle)
            }
        }
    }
}

fn pixel_at(mat: &Mat, point: Point) -> Vec4b {
    *mat.at_pt::<Vec4b>(point)
        .unwrap_or_else(|_| panic!("unable to read pixel at {:?}", point))
}

fn anchor_at(mat: &Mat, offset: Point, size: usize, sign: i32) -> Option<(Point, Vec4b)> {
    (0..size).find_map(|i| {
        let value = sign * i as i32;
        let diag = offset + Point::new(value, value);
        let pixel = pixel_at(mat, diag);
        if pixel.iter().all(|v| *v >= 170) {
            Some((diag, pixel))
        } else {
            None
        }
    })
}
