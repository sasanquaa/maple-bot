use std::collections::HashMap;

use log::debug;
use opencv::{
    core::{MatTraitConst, Point, Rect, Vec4b},
    prelude::Mat,
};
use strsim::normalized_damerau_levenshtein;

use crate::{
    context::{Context, Contextual, ControlFlow},
    database::{Action, ActionKey, ActionMove, Minimap as MinimapData, query_maps, upsert_map},
    detect::Detector,
};

const MINIMAP_CHANGE_TIMEOUT: u32 = 200;
const MINIMAP_BORDER_WHITENESS_THRESHOLD: u8 = 170;
const MINIMAP_DETECT_RUNE_INTERVAL_TICKS: u32 = 305;
const MINIMAP_DETECT_ELITE_BOSS_INTERVAL_TICKS: u32 = 305;

#[derive(Debug, Default)]
pub struct MinimapState {
    pub data: MinimapData,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
struct Anchors {
    tl: (Point, Vec4b),
    br: (Point, Vec4b),
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct MinimapIdle {
    anchors: Anchors,
    pub bbox: Rect,
    pub scale_w: f32,
    pub scale_h: f32,
    pub partially_overlapping: bool,
    pub rune: Option<Point>,
    rune_interval: u32,
    pub has_elite_boss: bool,
    has_elite_boss_interval: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum Minimap {
    Idle(MinimapIdle),
    Detecting,
    Timeout(u32),
}

impl Contextual for Minimap {
    type Persistent = MinimapState;

    fn update(
        self,
        _: &Context,
        detector: &mut impl Detector,
        state: &mut MinimapState,
    ) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, detector, state))
    }
}

#[inline]
fn update_context(
    contextual: Minimap,
    detector: &mut impl Detector,
    state: &mut MinimapState,
) -> Minimap {
    match contextual {
        Minimap::Detecting => {
            let Some((contextual, data)) = update_detecting_context(detector) else {
                return Minimap::Timeout(0);
            };
            state.data = data;
            contextual
        }
        Minimap::Idle(idle) => update_idle_context(detector, idle).unwrap_or(Minimap::Timeout(0)),
        // stalling for a bit before re-detecting
        // maybe useful for dragging
        Minimap::Timeout(timeout) => {
            let timeout = timeout + 1;
            if timeout >= MINIMAP_CHANGE_TIMEOUT {
                return Minimap::Detecting;
            }
            Minimap::Timeout(timeout)
        }
    }
}

fn update_detecting_context(detector: &mut impl Detector) -> Option<(Minimap, MinimapData)> {
    let bbox = detector
        .detect_minimap(MINIMAP_BORDER_WHITENESS_THRESHOLD)
        .ok()?;
    let name = detector.detect_minimap_name(bbox).ok()?;
    let size = bbox.width.min(bbox.height) as usize;
    let tl = anchor_at(detector.mat(), bbox.tl(), size, 1)?;
    let br = anchor_at(detector.mat(), bbox.br(), size, -1)?;
    let anchors = Anchors { tl, br };
    let (data, scale_w, scale_h) = get_data_for_minimap(&bbox, &name)?;
    debug!(target: "minimap", "anchor points: {:?}", anchors);
    Some((
        Minimap::Idle(MinimapIdle {
            anchors,
            bbox,
            scale_w,
            scale_h,
            partially_overlapping: false,
            rune: None,
            rune_interval: 0,
            has_elite_boss: false,
            has_elite_boss_interval: 0,
        }),
        data,
    ))
}

fn update_idle_context(detector: &mut impl Detector, idle: MinimapIdle) -> Option<Minimap> {
    let MinimapIdle {
        anchors,
        bbox,
        scale_w,
        scale_h,
        ..
    } = idle;
    let tl_pixel = pixel_at(detector.mat(), anchors.tl.0)?;
    let br_pixel = pixel_at(detector.mat(), anchors.br.0)?;
    let tl_match = tl_pixel == anchors.tl.1;
    let br_match = br_pixel == anchors.br.1;
    if !tl_match && !br_match {
        debug!(
            target: "minimap",
            "anchor pixels mismatch: {:?} != {:?}",
            (tl_pixel, br_pixel),
            (anchors.tl.1, anchors.br.1)
        );
        return Some(Minimap::Timeout(0));
    }
    let mut rune = idle.rune;
    if idle.rune_interval % MINIMAP_DETECT_RUNE_INTERVAL_TICKS == 0 {
        rune = detector.detect_minimap_rune(bbox).ok().map(|rune| {
            let tl = rune.tl() - bbox.tl();
            let br = rune.br() - bbox.tl();
            let x = ((tl.x + br.x) / 2) as f32 / scale_w;
            let y = (bbox.height - br.y + 1) as f32 / scale_h;
            let point = Point::new(x as i32, y as i32);
            debug!(target: "minimap", "detected rune at {point:?}");
            point
        });
    }
    let mut has_elite_boss = idle.has_elite_boss;
    if idle.has_elite_boss_interval % MINIMAP_DETECT_ELITE_BOSS_INTERVAL_TICKS == 0 {
        has_elite_boss = detector.detect_elite_boss_bar();
    }
    Some(Minimap::Idle(MinimapIdle {
        partially_overlapping: (tl_match && !br_match) || (!tl_match && br_match),
        rune,
        rune_interval: (idle.rune_interval + 1) % MINIMAP_DETECT_RUNE_INTERVAL_TICKS,
        has_elite_boss,
        has_elite_boss_interval: (idle.has_elite_boss_interval + 1)
            % MINIMAP_DETECT_ELITE_BOSS_INTERVAL_TICKS,
        ..idle
    }))
}

fn get_data_for_minimap(bbox: &Rect, name: &str) -> Option<(MinimapData, f32, f32)> {
    const MATCH_SCORE: f64 = 0.9;

    let candidate = query_maps().ok()?.into_iter().find_map(|map| {
        if normalized_damerau_levenshtein(name, &map.name) >= MATCH_SCORE {
            debug!(target: "minimap", "possible candidate {map:?}");
            let detected_numbers = name
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<Vec<_>>();
            let map_numbers = map
                .name
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<Vec<_>>();
            if detected_numbers == map_numbers {
                debug!(target: "minimap", "matched candidate found {map:?}");
                return Some(map);
            }
        }
        None
    });
    match candidate {
        Some(mut map) => {
            match (bbox.width, bbox.height, map.width, map.height) {
                // in resolution above 1366 x 768 with Default Ratio applied, the UI is enlarged
                // so try to prefer smaller resolution if detectable
                // smaller resolution also helps with template matching as the template
                // for player is created in 1024 x 768
                (b_w, b_h, m_w, m_h) if b_w < m_w && b_h < m_h => {
                    debug!(target: "minimap", "smaller minimap version detected (Ideal Ratio or resolution below 1366 x 768)");
                    let w_ratio = b_w as f32 / m_w as f32;
                    let h_ratio = b_h as f32 / m_h as f32;
                    map.actions.values_mut().flatten().for_each(|action| {
                        match action {
                            Action::Move(ActionMove { position, .. }) => {
                                position.x = (position.x as f32 * w_ratio) as i32;
                                position.y = (position.y as f32 * h_ratio) as i32;
                            }
                            Action::Key(ActionKey { position, .. }) => {
                                if let Some(position) = position {
                                    position.x = (position.x as f32 * w_ratio) as i32;
                                    position.y = (position.y as f32 * h_ratio) as i32;
                                }
                            }
                        };
                    });
                    map.width = b_w;
                    map.height = b_h;
                    upsert_map(&mut map).ok()?;
                    Some((map, 1.0, 1.0))
                }
                (b_w, b_h, m_w, m_h) if b_w > m_w && b_h > m_h => {
                    let w_ratio = b_w as f32 / m_w as f32;
                    let h_ratio = b_h as f32 / m_h as f32;
                    debug!(target: "minimap", "UI enlarged by {w_ratio} / {h_ratio} (Default Ratio)");
                    Some((map, w_ratio, h_ratio))
                }
                // TODO: map that has "smaller" version that requires click to expand?
                // TODO: check slight differences in width or height?
                _ => Some((map, 1.0, 1.0)),
            }
        }
        None => {
            let mut map = MinimapData {
                id: None,
                name: name.to_string(),
                width: bbox.width,
                height: bbox.height,
                actions: HashMap::new(),
            };
            upsert_map(&mut map).ok()?;
            debug!(target: "minimap", "new minimap data detected {map:?}");
            Some((map, 1.0, 1.0))
        }
    }
}

#[inline]
fn pixel_at(mat: &Mat, point: Point) -> Option<Vec4b> {
    mat.at_pt::<Vec4b>(point).ok().copied()
}

#[inline]
fn anchor_at(mat: &Mat, offset: Point, size: usize, sign: i32) -> Option<(Point, Vec4b)> {
    (0..size).find_map(|i| {
        let value = sign * i as i32;
        let diag = offset + Point::new(value, value);
        let pixel = pixel_at(mat, diag)?;
        if pixel
            .iter()
            .all(|v| *v >= MINIMAP_BORDER_WHITENESS_THRESHOLD)
        {
            Some((diag, pixel))
        } else {
            None
        }
    })
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::detect::MockDetector;
//     use mockall::predicate::eq;
//     use opencv::core::{Mat, MatExprTraitConst, MatTrait, Point, Rect, Vec4b};

//     fn create_test_mat() -> (Mat, Point, Point) {
//         let mut mat = Mat::zeros(100, 100, opencv::core::CV_8UC4)
//             .unwrap()
//             .to_mat()
//             .unwrap();
//         let tr = Point::new(10, 10);
//         let bl = Point::new(90, 90);
//         *mat.at_pt_mut::<Vec4b>(tr).unwrap() = Vec4b::all(255);
//         *mat.at_pt_mut::<Vec4b>(bl).unwrap() = Vec4b::all(255);
//         (mat, tr, bl)
//     }

//     #[test]
//     fn minimap_detecting_to_idle() {
//         let mut detector = MockDetector::new();
//         let mut state = MinimapState::default();
//         let bbox = Rect::new(0, 0, 100, 100);
//         detector
//             .expect_detect_minimap()
//             .with(eq(MINIMAP_BORDER_WHITENESS_THRESHOLD))
//             .returning(move |_| Ok(bbox));
//         detector
//             .expect_detect_minimap_name()
//             .with(eq(bbox))
//             .returning(|_| Ok("TestMap".to_string()));

//         let (mat, tl, br) = create_test_mat();
//         detector.expect_mat().return_const(mat);

//         let minimap = update_context(Minimap::Detecting, &mut detector, &mut state);
//         assert!(matches!(minimap, Minimap::Idle(_)));
//         assert_eq!(minimap.)
//     }

// #[test]
// fn test_minimap_idle_to_timeout() {
//     let mut detector = MockDetector::new();
//     let mut state = MinimapState::default();

//     // Simulate anchor points mismatch
//     let mat = create_test_mat();
//     detector.expect_mat().return_const(mat);

//     let idle = MinimapIdle {
//         anchors: Anchors {
//             tl: (Point::new(10, 10), Vec4b::all(255)),
//             br: (Point::new(90, 90), Vec4b::all(0)), // Mismatch
//         },
//         bbox: Rect::new(0, 0, 100, 100),
//         scale_w: 1.0,
//         scale_h: 1.0,
//         partially_overlapping: false,
//         rune: None,
//         rune_detect_interval: 0,
//     };

//     let minimap = update_context(Minimap::Idle(idle), &mut detector, &mut state);
//     assert!(matches!(minimap, Minimap::Timeout(0)));
// }

// #[test]
// fn test_minimap_timeout_to_detecting() {
//     let mut detector = MockDetector::new();
//     let mut state = MinimapState::default();

//     // Simulate timeout reaching the limit
//     let minimap = update_context(
//         Minimap::Timeout(MINIMAP_CHANGE_TIMEOUT - 1),
//         &mut detector,
//         &mut state,
//     );
//     assert!(matches!(minimap, Minimap::Detecting));
// }

// #[test]
// fn test_minimap_rune_detection() {
//     let mut detector = MockDetector::new();
//     let mut state = MinimapState::default();

//     // Simulate rune detection
//     let bbox = Rect::new(0, 0, 100, 100);
//     let rune_bbox = Rect::new(40, 40, 20, 20);
//     detector
//         .expect_detect_minimap_rune()
//         .withf(move |b| *b == bbox)
//         .returning(move || Ok(rune_bbox));

//     let mat = create_test_mat();
//     detector.expect_mat().return_const(mat);

//     let idle = MinimapIdle {
//         anchors: Anchors {
//             tl: (Point::new(10, 10), Vec4b::all(255)),
//             br: (Point::new(90, 90), Vec4b::all(255)),
//         },
//         bbox,
//         scale_w: 1.0,
//         scale_h: 1.0,
//         partially_overlapping: false,
//         rune: None,
//         rune_detect_interval: MINIMAP_DETECT_RUNE_INTERVAL_TICKS - 1,
//     };

//     let minimap = update_context(Minimap::Idle(idle), &mut detector, &mut state);
//     if let Minimap::Idle(idle) = minimap {
//         assert!(idle.rune.is_some());
//     } else {
//         panic!("Expected Minimap::Idle");
//     }
// }

// #[test]
// fn test_minimap_data_scaling() {
//     let mut detector = MockDetector::new();
//     let mut state = MinimapState::default();

//     // Simulate detecting a minimap
//     let bbox = Rect::new(0, 0, 100, 100);
//     detector
//         .expect_detect_minimap()
//         .withf(move |threshold| *threshold == MINIMAP_BORDER_WHITENESS_THRESHOLD)
//         .returning(move || Ok(bbox));

//     // Simulate detecting a minimap name
//     detector
//         .expect_detect_minimap_name()
//         .withf(move |b| *b == bbox)
//         .returning(|_| Ok("TestMap".to_string()));

//     // Simulate anchor points
//     let mat = create_test_mat();
//     detector.expect_mat().return_const(mat);

//     // Simulate querying maps
//     let mut map = MinimapData {
//         id: None,
//         name: "TestMap".to_string(),
//         width: 200,
//         height: 200,
//         actions: HashMap::new(),
//     };
//     map.actions
//         .insert("test".to_string(), vec![Action::Move(ActionMove {
//             position: Point::new(100, 100),
//             ..Default::default()
//         })]);

//     // Mock the database query
//     let _ = query_maps().and_then(|maps| {
//         maps.into_iter().find(|m| m.name == "TestMap").map(|m| {
//             assert_eq!(m.width, 200);
//             assert_eq!(m.height, 200);
//         })
//     });

//     let minimap = update_context(Minimap::Detecting, &mut detector, &mut state);
//     assert!(matches!(minimap, Minimap::Idle(_)));
// }
// }
