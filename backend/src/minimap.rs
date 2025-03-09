use std::collections::HashMap;

use log::debug;
use opencv::{
    core::{MatTraitConst, Point, Rect, Vec4b},
    prelude::Mat,
};
use strsim::normalized_damerau_levenshtein;

use crate::{
    context::{Context, Contextual, ControlFlow, Timeout, update_with_timeout},
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
#[cfg_attr(test, derive(Default, PartialEq))]
struct Anchors {
    tl: (Point, Vec4b),
    br: (Point, Vec4b),
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct MinimapIdle {
    /// The two anchors top left and bottom right of the minimap.
    /// They are just two fixed pixels.
    anchors: Anchors,
    /// The bounding box of the minimap.
    pub bbox: Rect,
    /// Whether the UI width is scaled or not depending on the saved data
    pub scale_w: f32,
    /// Whether the UI height is scaled or not depending on the saved data
    pub scale_h: f32,
    /// Approximates whether the minimap UI has other UI partially overlapping it
    pub partially_overlapping: bool,
    /// The rune position
    pub rune: Option<Point>,
    /// Timeout for detecting rune in a fixed interval
    rune_timeout: Timeout,
    /// Timeout for detecting elite boss in a fixed interval
    pub has_elite_boss: bool,
    has_elite_boss_timeout: Timeout,
}

#[derive(Clone, Copy, Debug)]
pub enum Minimap {
    Idle(MinimapIdle),
    Detecting,
    Timeout(Timeout),
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
                return Minimap::Timeout(Timeout::default());
            };
            state.data = data;
            contextual
        }
        Minimap::Idle(idle) => {
            update_idle_context(detector, idle).unwrap_or(Minimap::Timeout(Timeout::default()))
        }
        // stalling for a bit before re-detecting
        // maybe useful for dragging
        Minimap::Timeout(timeout) => update_with_timeout(
            timeout,
            MINIMAP_CHANGE_TIMEOUT,
            (),
            |_, timeout| Minimap::Timeout(timeout),
            |_| Minimap::Detecting,
            |_, timeout| Minimap::Timeout(timeout),
        ),
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
            rune_timeout: Timeout::default(),
            has_elite_boss: false,
            has_elite_boss_timeout: Timeout::default(),
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
        rune,
        rune_timeout,
        has_elite_boss,
        has_elite_boss_timeout,
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
        return Some(Minimap::Timeout(Timeout::default()));
    }
    let (rune, rune_timeout) = update_with_timeout(
        rune_timeout,
        MINIMAP_DETECT_RUNE_INTERVAL_TICKS,
        (),
        |_, timeout| {
            (
                detector
                    .detect_minimap_rune(bbox)
                    .ok()
                    .map(|rune| center_of_rune(rune, bbox, scale_w, scale_h)),
                timeout,
            )
        },
        |_| (rune, Timeout::default()),
        |_, timeout| (rune, timeout),
    );
    let (has_elite_boss, has_elite_boss_timeout) = update_with_timeout(
        has_elite_boss_timeout,
        MINIMAP_DETECT_ELITE_BOSS_INTERVAL_TICKS,
        (),
        |_, timeout| (detector.detect_elite_boss_bar(), timeout),
        |_| (has_elite_boss, Timeout::default()),
        |_, timeout| (has_elite_boss, timeout),
    );
    Some(Minimap::Idle(MinimapIdle {
        partially_overlapping: (tl_match && !br_match) || (!tl_match && br_match),
        rune,
        rune_timeout,
        has_elite_boss,
        has_elite_boss_timeout,
        ..idle
    }))
}

fn get_data_for_minimap(bbox: &Rect, name: &str) -> Option<(MinimapData, f32, f32)> {
    const MATCH_SCORE: f64 = 0.9;

    // TODO: Mock this
    if cfg!(test) {
        return Some((
            MinimapData {
                id: None,
                name: name.to_string(),
                width: bbox.width,
                height: bbox.height,
                actions: HashMap::new(),
            },
            1.0,
            1.0,
        ));
    }

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
fn center_of_rune(rune: Rect, bbox: Rect, scale_w: f32, scale_h: f32) -> Point {
    let tl = rune.tl() - bbox.tl();
    let br = rune.br() - bbox.tl();
    let x = ((tl.x + br.x) / 2) as f32 / scale_w;
    let y = (bbox.height - br.y + 1) as f32 / scale_h;
    let point = Point::new(x as i32, y as i32);
    debug!(target: "minimap", "detected rune at {point:?}");
    point
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::MockDetector;
    use mockall::predicate::eq;
    use opencv::core::{Mat, MatExprTraitConst, MatTrait, Point, Rect, Vec4b};

    fn create_test_mat() -> (Mat, Anchors) {
        let mut mat = Mat::zeros(100, 100, opencv::core::CV_8UC4)
            .unwrap()
            .to_mat()
            .unwrap();
        let pixel = Vec4b::all(255);
        let tl = Point::new(10, 10);
        let br = Point::new(90, 90);
        *mat.at_pt_mut::<Vec4b>(tl).unwrap() = Vec4b::all(255);
        *mat.at_pt_mut::<Vec4b>(br).unwrap() = Vec4b::all(255);
        (mat, Anchors {
            tl: (tl, pixel),
            br: (br, pixel),
        })
    }

    #[test]
    fn minimap_detecting_to_idle() {
        let mut detector = MockDetector::new();
        let mut state = MinimapState::default();
        let (mat, anchors) = create_test_mat();
        let bbox = Rect::new(0, 0, 100, 100);
        let data = MinimapData {
            id: None,
            name: "TestMap".to_string(),
            width: bbox.width,
            height: bbox.height,
            actions: HashMap::new(),
        };
        detector
            .expect_detect_minimap()
            .with(eq(MINIMAP_BORDER_WHITENESS_THRESHOLD))
            .returning(move |_| Ok(bbox));
        detector
            .expect_detect_minimap_name()
            .with(eq(bbox))
            .returning(|_| Ok("TestMap".to_string()));
        detector.expect_mat().return_const(mat);

        let minimap = update_context(Minimap::Detecting, &mut detector, &mut state);
        assert!(matches!(minimap, Minimap::Idle(_)));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.anchors, anchors);
                assert_eq!(idle.bbox, bbox);
                assert!(!idle.partially_overlapping);
                assert_eq!(state.data, data);
                assert_eq!(idle.rune, None);
                assert_eq!(idle.rune_timeout, Timeout::default());
                assert!(!idle.has_elite_boss);
                assert_eq!(idle.has_elite_boss_timeout, Timeout::default());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn minimap_idle_to_timeout() {
        let mut detector = MockDetector::new();
        let mut state = MinimapState::default();
        let (mat, _) = create_test_mat();
        let idle = MinimapIdle {
            anchors: Anchors {
                tl: (Point::new(10, 10), Vec4b::all(0)),
                br: (Point::new(90, 90), Vec4b::all(0)),
            },
            bbox: Rect::new(0, 0, 100, 100),
            scale_w: 1.0,
            scale_h: 1.0,
            partially_overlapping: false,
            rune: None,
            rune_timeout: Timeout {
                current: 0,
                started: true,
            },
            has_elite_boss: false,
            has_elite_boss_timeout: Timeout {
                current: 0,
                started: true,
            },
        };
        detector.expect_mat().return_const(mat);

        let minimap = update_context(Minimap::Idle(idle), &mut detector, &mut state);
        assert!(matches!(minimap, Minimap::Timeout(_)));
    }

    #[test]
    fn minimap_timeout_to_detecting() {
        let mut detector = MockDetector::new();
        let mut state = MinimapState::default();

        let minimap = update_context(
            Minimap::Timeout(Timeout {
                current: MINIMAP_CHANGE_TIMEOUT,
                started: true,
            }),
            &mut detector,
            &mut state,
        );
        assert!(matches!(minimap, Minimap::Detecting));
    }

    #[test]
    fn minimap_idle_rune_detection() {
        let mut detector = MockDetector::new();
        let mut state = MinimapState::default();
        let bbox = Rect::new(0, 0, 100, 100);
        let (mat, anchors) = create_test_mat();
        let rune_bbox = Rect::new(40, 40, 20, 20);
        detector.expect_mat().return_const(mat);
        detector
            .expect_detect_minimap_rune()
            .withf(move |b| *b == bbox)
            .returning(move |_| Ok(rune_bbox));

        let idle = MinimapIdle {
            anchors,
            bbox,
            scale_w: 1.0,
            scale_h: 1.0,
            partially_overlapping: false,
            rune: None,
            rune_timeout: Timeout::default(),
            has_elite_boss: false,
            has_elite_boss_timeout: Timeout {
                current: 0,
                started: true,
            },
        };

        let minimap = update_context(Minimap::Idle(idle), &mut detector, &mut state);
        assert!(matches!(minimap, Minimap::Idle(_)));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.rune, Some(center_of_rune(rune_bbox, bbox, 1.0, 1.0)));
                assert_eq!(idle.rune_timeout, Timeout {
                    current: 0,
                    started: true
                });
            }
            _ => unreachable!(),
        }
    }
}
