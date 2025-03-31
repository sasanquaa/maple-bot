use anyhow::{Result, anyhow};
use log::debug;
use opencv::{
    core::{MatTraitConst, Point, Rect, Vec4b},
    prelude::Mat,
};
use strsim::normalized_damerau_levenshtein;

use crate::{
    array::Array,
    context::{Context, Contextual, ControlFlow},
    database::{Action, ActionKey, ActionMove, Minimap as MinimapData, query_maps, upsert_map},
    detect::Detector,
    pathing::{Platform, PlatformWithNeighbors, find_neighbors},
    player::{
        DOUBLE_JUMP_THRESHOLD, PLAYER_GRAPPLING_MAX_THRESHOLD, PLAYER_JUMP_THRESHOLD, Player,
    },
    task::{Task, Update, update_task_repeatable},
};

const MINIMAP_BORDER_WHITENESS_THRESHOLD: u8 = 170;

type TaskData = (Anchors, Rect, MinimapData, f32, f32);

#[derive(Debug, Default)]
pub struct MinimapState {
    pub data: MinimapData,
    data_task: Option<Task<Result<TaskData>>>,
    rune_task: Option<Task<Result<Point>>>,
    portals_task: Option<Task<Result<Vec<Rect>>>>,
    has_elite_boss_task: Option<Task<Result<bool>>>,
    pub update_platforms: bool,
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
    /// Whether there is an elite boss
    /// This does not belong to minimap though...
    pub has_elite_boss: bool,
    /// The portal positions
    /// Praying each night that there won't be more than 16 portals...
    // initially it is only 8 until it crashes at Henesys with 10 portals smh
    pub portals: Array<Rect, 16>,
    pub platforms: Array<PlatformWithNeighbors, 24>,
}

#[derive(Clone, Copy, Debug)]
pub enum Minimap {
    Detecting,
    Idle(MinimapIdle),
}

impl Contextual for Minimap {
    type Persistent = MinimapState;

    fn update(
        self,
        context: &Context,
        detector: &impl Detector,
        state: &mut MinimapState,
    ) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, context, detector, state))
    }
}

#[inline]
fn update_context(
    contextual: Minimap,
    context: &Context,
    detector: &impl Detector,
    state: &mut MinimapState,
) -> Minimap {
    match contextual {
        Minimap::Detecting => update_detecting_context(detector, state),
        Minimap::Idle(idle) => {
            update_idle_context(context, detector, state, idle).unwrap_or(Minimap::Detecting)
        }
    }
}

fn update_detecting_context(detector: &impl Detector, state: &mut MinimapState) -> Minimap {
    let detector = detector.clone();
    let Update::Complete(Ok((anchors, bbox, data, scale_w, scale_h))) =
        update_task_repeatable(2000, &mut state.data_task, move || {
            let bbox = detector.detect_minimap(MINIMAP_BORDER_WHITENESS_THRESHOLD)?;
            let name = detector.detect_minimap_name(bbox)?;
            let size = bbox.width.min(bbox.height) as usize;
            let tl = anchor_at(detector.mat(), bbox.tl(), size, 1)?;
            let br = anchor_at(detector.mat(), bbox.br(), size, -1)?;
            let anchors = Anchors { tl, br };
            let (data, scale_w, scale_h) = query_data_for_minimap(&bbox, &name)?;
            debug!(target: "minimap", "anchor points: {:?}", anchors);
            Ok((anchors, bbox, data, scale_w, scale_h))
        })
    else {
        return Minimap::Detecting;
    };
    state.data = data;
    state.update_platforms = false;
    state.rune_task = None;
    state.has_elite_boss_task = None;
    Minimap::Idle(MinimapIdle {
        anchors,
        bbox,
        scale_w,
        scale_h,
        partially_overlapping: false,
        rune: None,
        has_elite_boss: false,
        portals: Array::new(),
        platforms: platforms_from_data(&state.data),
    })
}

fn update_idle_context(
    context: &Context,
    detector: &impl Detector,
    state: &mut MinimapState,
    idle: MinimapIdle,
) -> Option<Minimap> {
    let MinimapIdle {
        anchors,
        bbox,
        scale_w,
        scale_h,
        rune,
        has_elite_boss,
        mut portals,
        mut platforms,
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
        return None;
    }
    let rune_detector = detector.clone();
    let rune_update = if matches!(context.player, Player::SolvingRune(_)) && rune.is_some() {
        Update::Pending
    } else {
        update_task_repeatable(10000, &mut state.rune_task, move || {
            rune_detector
                .detect_minimap_rune(bbox)
                .map(|rune| center_of_bbox(rune, bbox, scale_w, scale_h))
        })
    };
    let rune = match rune_update {
        Update::Complete(rune) => rune.ok(),
        Update::Pending => rune,
    };
    let elite_boss_detector = detector.clone();
    let elite_boss_update =
        update_task_repeatable(10000, &mut state.has_elite_boss_task, move || {
            Ok(elite_boss_detector.detect_elite_boss_bar())
        });
    let has_elite_boss = match elite_boss_update {
        Update::Complete(has_elite_boss) => has_elite_boss.unwrap(),
        Update::Pending => has_elite_boss,
    };
    let portals_detector = detector.clone();
    let portals_update = update_task_repeatable(10000, &mut state.portals_task, move || {
        portals_detector.detect_minimap_portals(bbox)
    });
    let portals = match portals_update {
        Update::Complete(Ok(vec)) => {
            if portals.len() < vec.len() {
                portals.consume(vec.into_iter().map(|portal| {
                    Rect::new(
                        portal.x,
                        bbox.height - portal.y,
                        portal.width,
                        portal.height,
                    )
                }));
            }
            portals
        }
        Update::Complete(_) | Update::Pending => portals,
    };
    // TODO: any better way to read persistent state in other contextual?
    if state.update_platforms {
        state.update_platforms = false;
        platforms = platforms_from_data(&state.data);
    }

    Some(Minimap::Idle(MinimapIdle {
        partially_overlapping: (tl_match && !br_match) || (!tl_match && br_match),
        rune,
        has_elite_boss,
        portals,
        platforms,
        ..idle
    }))
}

fn query_data_for_minimap(bbox: &Rect, name: &str) -> Result<(MinimapData, f32, f32)> {
    const MATCH_SCORE: f64 = 0.9;

    // TODO: Mock this
    if cfg!(test) {
        return Ok((
            MinimapData {
                name: name.to_string(),
                width: bbox.width,
                height: bbox.height,
                ..MinimapData::default()
            },
            1.0,
            1.0,
        ));
    }

    let candidate = query_maps()?.into_iter().find_map(|map| {
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
                    upsert_map(&mut map)?;
                    Ok((map, 1.0, 1.0))
                }
                (b_w, b_h, m_w, m_h) if b_w > m_w && b_h > m_h => {
                    let w_ratio = b_w as f32 / m_w as f32;
                    let h_ratio = b_h as f32 / m_h as f32;
                    debug!(target: "minimap", "UI enlarged by {w_ratio} / {h_ratio} (Default Ratio)");
                    Ok((map, w_ratio, h_ratio))
                }
                // TODO: map that has "smaller" version that requires click to expand?
                // TODO: check slight differences in width or height?
                _ => Ok((map, 1.0, 1.0)),
            }
        }
        None => {
            let mut map = MinimapData {
                name: name.to_string(),
                width: bbox.width,
                height: bbox.height,
                ..MinimapData::default()
            };
            upsert_map(&mut map)?;
            debug!(target: "minimap", "new minimap data detected {map:?}");
            Ok((map, 1.0, 1.0))
        }
    }
}

fn platforms_from_data(minimap: &MinimapData) -> Array<PlatformWithNeighbors, 24> {
    Array::from_iter(find_neighbors(
        &minimap
            .platforms
            .iter()
            .copied()
            .map(|platform| Platform::from(platform))
            .collect::<Vec<_>>(),
        DOUBLE_JUMP_THRESHOLD,
        PLAYER_JUMP_THRESHOLD,
        PLAYER_GRAPPLING_MAX_THRESHOLD,
    ))
}

#[inline]
fn center_of_bbox(bbox: Rect, minimap: Rect, scale_w: f32, scale_h: f32) -> Point {
    let tl = bbox.tl();
    let br = bbox.br();
    let x = ((tl.x + br.x) / 2) as f32 / scale_w;
    let y = (minimap.height - br.y + 1) as f32 / scale_h;
    let point = Point::new(x as i32, y as i32);
    point
}

#[inline]
fn pixel_at(mat: &Mat, point: Point) -> Option<Vec4b> {
    mat.at_pt::<Vec4b>(point).ok().copied()
}

#[inline]
fn anchor_at(mat: &Mat, offset: Point, size: usize, sign: i32) -> Result<(Point, Vec4b)> {
    (0..size)
        .find_map(|i| {
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
        .ok_or(anyhow!("anchor not found"))
}

#[cfg(test)]
mod tests {
    use std::{assert_matches::assert_matches, time::Duration};

    use mockall::predicate::eq;
    use opencv::core::{Mat, MatExprTraitConst, MatTrait, Point, Rect, Vec4b};
    use tokio::time;

    use super::*;
    use crate::detect::MockDetector;

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

    fn create_mock_detector() -> (MockDetector, Rect, Anchors, MinimapData, Rect) {
        let mut detector = MockDetector::new();
        let (mat, anchors) = create_test_mat();
        let bbox = Rect::new(0, 0, 100, 100);
        let rune_bbox = Rect::new(40, 40, 20, 20);
        let data = MinimapData {
            name: "TestMap".to_string(),
            width: bbox.width,
            height: bbox.height,
            ..MinimapData::default()
        };
        detector
            .expect_detect_minimap_rune()
            .withf(move |b| *b == bbox)
            .returning(move |_| Ok(rune_bbox));
        detector
            .expect_clone()
            .returning(|| create_mock_detector().0);
        detector
            .expect_detect_minimap()
            .with(eq(MINIMAP_BORDER_WHITENESS_THRESHOLD))
            .returning(move |_| Ok(bbox));
        detector
            .expect_detect_minimap_name()
            .with(eq(bbox))
            .returning(|_| Ok("TestMap".to_string()));
        detector.expect_mat().return_const(mat);
        (detector, bbox, anchors, data, rune_bbox)
    }

    async fn advance_task(
        contextual: Minimap,
        detector: &impl Detector,
        state: &mut MinimapState,
    ) -> Minimap {
        let context = Context::default();
        let completed = |state: &MinimapState| {
            if matches!(contextual, Minimap::Idle(_)) {
                state.rune_task.as_ref().unwrap().completed()
            } else {
                state.data_task.as_ref().unwrap().completed()
            }
        };
        let mut minimap = update_context(contextual, &context, detector, state);
        while !completed(state) {
            minimap = update_context(minimap, &context, detector, state);
            time::advance(Duration::from_millis(1000)).await;
        }
        minimap
    }

    #[tokio::test(start_paused = true)]
    async fn minimap_detecting_to_idle() {
        let mut state = MinimapState::default();
        let (detector, bbox, anchors, data, _) = create_mock_detector();

        let minimap = advance_task(Minimap::Detecting, &detector, &mut state).await;
        assert_matches!(minimap, Minimap::Idle(_));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.anchors, anchors);
                assert_eq!(idle.bbox, bbox);
                assert!(!idle.partially_overlapping);
                assert_eq!(state.data, data);
                assert_eq!(idle.rune, None);
                assert!(!idle.has_elite_boss);
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn minimap_idle_rune_detection() {
        let mut state = MinimapState::default();
        let (detector, bbox, anchors, _, rune_bbox) = create_mock_detector();

        let idle = MinimapIdle {
            anchors,
            bbox,
            scale_w: 1.0,
            scale_h: 1.0,
            partially_overlapping: false,
            rune: None,
            has_elite_boss: false,
            portals: Array::new(),
            platforms: Array::new(),
        };

        let minimap = advance_task(Minimap::Idle(idle), &detector, &mut state).await;
        assert_matches!(minimap, Minimap::Idle(_));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.rune, Some(center_of_bbox(rune_bbox, bbox, 1.0, 1.0)));
            }
            _ => unreachable!(),
        }
    }
}
