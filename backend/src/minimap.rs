use anyhow::{Result, anyhow};
use log::debug;
use opencv::core::{MatTraitConst, Point, Rect, Vec4b};

use crate::{
    array::Array,
    context::{Context, Contextual, ControlFlow},
    database::Minimap as MinimapData,
    detect::Detector,
    pathing::{
        MAX_PLATFORMS_COUNT, Platform, PlatformWithNeighbors, find_neighbors, find_platforms_bound,
    },
    player::{DOUBLE_JUMP_THRESHOLD, GRAPPLING_MAX_THRESHOLD, JUMP_THRESHOLD, Player},
    task::{Task, Update, update_task_repeatable},
};

const MINIMAP_BORDER_WHITENESS_THRESHOLD: u8 = 170;

#[derive(Debug, Default)]
pub struct MinimapState {
    data: Option<MinimapData>,
    data_task: Option<Task<Result<(Anchors, Rect)>>>,
    rune_task: Option<Task<Result<Point>>>,
    portals_task: Option<Task<Result<Vec<Rect>>>>,
    has_elite_boss_task: Option<Task<Result<bool>>>,
    update_platforms: bool,
}

impl MinimapState {
    pub fn data(&self) -> Option<&MinimapData> {
        self.data.as_ref()
    }

    pub fn set_data(&mut self, data: MinimapData) {
        self.data = Some(data);
        self.update_platforms = true;
    }
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
    /// The two anchors top left and bottom right of the minimap
    /// They are just two fixed pixels
    anchors: Anchors,
    /// The bounding box of the minimap.
    pub bbox: Rect,
    /// Whether the UI width is scaled or not depending on the saved data
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
    pub platforms: Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT>,
    pub platforms_bound: Option<Rect>,
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::large_enum_variant)] // There is only ever a single instance of Minimap
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
    let Update::Complete(Ok((anchors, bbox))) =
        update_task_repeatable(2000, &mut state.data_task, move || {
            let bbox = detector.detect_minimap(MINIMAP_BORDER_WHITENESS_THRESHOLD)?;
            let size = bbox.width.min(bbox.height) as usize;
            let tl = anchor_at(detector.mat(), bbox.tl(), size, 1)?;
            let br = anchor_at(detector.mat(), bbox.br(), size, -1)?;
            let anchors = Anchors { tl, br };
            debug!(target: "minimap", "anchor points: {:?}", anchors);
            Ok((anchors, bbox))
        })
    else {
        return Minimap::Detecting;
    };
    let (platforms, platforms_bound) = state
        .data
        .as_ref()
        .map(|data| platforms_from_data(bbox, data))
        .unwrap_or_default();
    state.update_platforms = false;
    state.rune_task = None;
    state.has_elite_boss_task = None;
    Minimap::Idle(MinimapIdle {
        anchors,
        bbox,
        partially_overlapping: false,
        rune: None,
        has_elite_boss: false,
        portals: Array::new(),
        platforms,
        platforms_bound,
    })
}

fn update_idle_context(
    context: &Context,
    detector: &impl Detector,
    state: &mut MinimapState,
    idle: MinimapIdle,
) -> Option<Minimap> {
    if matches!(context.player, Player::CashShopThenExit(_, _)) {
        return Some(Minimap::Idle(idle));
    }
    let MinimapIdle {
        anchors,
        bbox,
        rune,
        has_elite_boss,
        portals,
        mut platforms,
        mut platforms_bound,
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
    let partially_overlapping = (tl_match && !br_match) || (!tl_match && br_match);
    let rune = update_rune_task(context, state, detector, bbox, rune);
    let has_elite_boss = update_elite_boss_task(state, detector, has_elite_boss);
    let portals = update_portals_task(state, detector, portals, bbox);
    // TODO: any better way to read persistent state in other contextual?
    if state.update_platforms {
        let (updated_platforms, updated_bound) =
            platforms_from_data(bbox, state.data.as_mut().unwrap());
        state.update_platforms = false;
        platforms = updated_platforms;
        platforms_bound = updated_bound
    }

    Some(Minimap::Idle(MinimapIdle {
        partially_overlapping,
        rune,
        has_elite_boss,
        portals,
        platforms,
        platforms_bound,
        ..idle
    }))
}

#[inline]
fn update_rune_task(
    context: &Context,
    state: &mut MinimapState,
    detector: &impl Detector,
    minimap: Rect,
    rune: Option<Point>,
) -> Option<Point> {
    let detector = detector.clone();
    let update = if matches!(context.player, Player::SolvingRune(_)) && rune.is_some() {
        Update::Pending
    } else {
        update_task_repeatable(10000, &mut state.rune_task, move || {
            detector
                .detect_minimap_rune(minimap)
                .map(|rune| center_of_bbox(rune, minimap))
        })
    };
    match update {
        Update::Complete(rune) => rune.ok(),
        Update::Pending => rune,
    }
}

#[inline]
fn update_elite_boss_task(
    state: &mut MinimapState,
    detector: &impl Detector,
    has_elite_boss: bool,
) -> bool {
    let detector = detector.clone();
    let update = update_task_repeatable(10000, &mut state.has_elite_boss_task, move || {
        Ok(detector.detect_elite_boss_bar())
    });
    match update {
        Update::Complete(has_elite_boss) => has_elite_boss.unwrap(),
        Update::Pending => has_elite_boss,
    }
}

#[inline]
fn update_portals_task(
    state: &mut MinimapState,
    detector: &impl Detector,
    portals: Array<Rect, 16>,
    minimap: Rect,
) -> Array<Rect, 16> {
    let detector = detector.clone();
    let update = update_task_repeatable(5000, &mut state.portals_task, move || {
        detector.detect_minimap_portals(minimap)
    });
    match update {
        Update::Complete(Ok(vec)) if portals.len() < vec.len() => {
            Array::from_iter(vec.into_iter().map(|portal| {
                Rect::new(
                    portal.x,
                    minimap.height - portal.y,
                    portal.width,
                    portal.height,
                )
            }))
        }
        Update::Complete(_) | Update::Pending => portals,
    }
}

fn platforms_from_data(
    bbox: Rect,
    minimap: &MinimapData,
) -> (Array<PlatformWithNeighbors, 24>, Option<Rect>) {
    let platforms = Array::from_iter(find_neighbors(
        &minimap
            .platforms
            .iter()
            .copied()
            .map(Platform::from)
            .collect::<Vec<_>>(),
        DOUBLE_JUMP_THRESHOLD,
        JUMP_THRESHOLD,
        GRAPPLING_MAX_THRESHOLD,
    ));
    let bound = find_platforms_bound(bbox, &platforms);
    (platforms, bound)
}

#[inline]
fn center_of_bbox(bbox: Rect, minimap: Rect) -> Point {
    let tl = bbox.tl();
    let br = bbox.br();
    let x = (tl.x + br.x) / 2;
    let y = minimap.height - br.y + 1;
    Point::new(x, y)
}

#[inline]
fn pixel_at(mat: &impl MatTraitConst, point: Point) -> Option<Vec4b> {
    mat.at_pt::<Vec4b>(point).ok().copied()
}

#[inline]
fn anchor_at(
    mat: &impl MatTraitConst,
    offset: Point,
    size: usize,
    sign: i32,
) -> Result<(Point, Vec4b)> {
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
        (
            mat,
            Anchors {
                tl: (tl, pixel),
                br: (br, pixel),
            },
        )
    }

    fn create_mock_detector() -> (MockDetector, Rect, Anchors, Rect) {
        let mut detector = MockDetector::new();
        let (mat, anchors) = create_test_mat();
        let bbox = Rect::new(0, 0, 100, 100);
        let rune_bbox = Rect::new(40, 40, 20, 20);
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
        detector.expect_mat().return_const(mat.into());
        (detector, bbox, anchors, rune_bbox)
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
        let (detector, bbox, anchors, _) = create_mock_detector();

        let minimap = advance_task(Minimap::Detecting, &detector, &mut state).await;
        assert_matches!(minimap, Minimap::Idle(_));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.anchors, anchors);
                assert_eq!(idle.bbox, bbox);
                assert!(!idle.partially_overlapping);
                assert_eq!(state.data, None);
                assert_eq!(idle.rune, None);
                assert!(!idle.has_elite_boss);
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn minimap_idle_rune_detection() {
        let mut state = MinimapState::default();
        let (detector, bbox, anchors, rune_bbox) = create_mock_detector();

        let idle = MinimapIdle {
            anchors,
            bbox,
            partially_overlapping: false,
            rune: None,
            has_elite_boss: false,
            portals: Array::new(),
            platforms: Array::new(),
            platforms_bound: None,
        };

        let minimap = advance_task(Minimap::Idle(idle), &detector, &mut state).await;
        assert_matches!(minimap, Minimap::Idle(_));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.rune, Some(center_of_bbox(rune_bbox, bbox)));
            }
            _ => unreachable!(),
        }
    }
}
