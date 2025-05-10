use std::fmt;

use anyhow::{Result, anyhow};
use log::debug;
use opencv::core::{MatTraitConst, Point, Rect, Vec4b};

use crate::{
    array::Array,
    context::{Context, Contextual, ControlFlow},
    database::Minimap as MinimapData,
    detect::{Detector, OtherPlayerKind},
    network::NotificationKind,
    pathing::{
        MAX_PLATFORMS_COUNT, Platform, PlatformWithNeighbors, find_neighbors, find_platforms_bound,
    },
    player::{DOUBLE_JUMP_THRESHOLD, GRAPPLING_MAX_THRESHOLD, JUMP_THRESHOLD, Player},
    task::{Task, Update, update_detection_task},
};

const MINIMAP_BORDER_WHITENESS_THRESHOLD: u8 = 160;

#[derive(Debug, Default)]
pub struct MinimapState {
    data: Option<MinimapData>,
    minimap_task: Option<Task<Result<(Anchors, Rect)>>>,
    rune_task: Option<Task<Result<Point>>>,
    portals_task: Option<Task<Result<Vec<Rect>>>>,
    has_elite_boss_task: Option<Task<Result<bool>>>,
    has_guildie_player_task: Option<Task<Result<bool>>>,
    has_stranger_player_task: Option<Task<Result<bool>>>,
    has_friend_player_task: Option<Task<Result<bool>>>,
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
struct Threshold<T> {
    value: Option<T>,
    fail_count: u32,
    max_fail_count: u32,
}

impl<T> Threshold<T> {
    fn new(max_fail_count: u32) -> Self {
        Self {
            value: None,
            fail_count: 0,
            max_fail_count,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct MinimapIdle {
    /// The two anchors top left and bottom right of the minimap
    ///
    /// They are just two fixed pixels
    anchors: Anchors,
    /// The bounding box of the minimap.
    pub bbox: Rect,
    /// Whether the UI is being partially overlapped
    ///
    /// It is partially overlapped by other UIs if one of the anchor mismatches.
    pub partially_overlapping: bool,
    /// The rune position
    pub rune: Option<Point>,
    /// Rune detection fail count from having a rune
    ///
    /// If fail count reaches a threshold, rune is considered no longer on the minimap
    rune_fail_count: u32,
    /// Whether there is an elite boss
    ///
    /// This does not belong to minimap though...
    pub has_elite_boss: bool,
    /// Whether there is a guildie
    has_guildie_player: Threshold<bool>,
    /// Whether there is a stranger
    has_stranger_player: Threshold<bool>,
    /// Whether there is a friend
    has_friend_player: Threshold<bool>,
    /// The portal positions
    ///
    /// Praying each night that there won't be more than 16 portals...
    /// Initially, it is only 8 until it crashes at Henesys with 10 portals smh
    pub portals: Array<Rect, 16>,
    /// The user provided platforms
    pub platforms: Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT>,
    /// The largest rectangle containing all the platforms
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

    fn update(self, context: &Context, state: &mut MinimapState) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, context, state))
    }
}

#[inline]
fn update_context(contextual: Minimap, context: &Context, state: &mut MinimapState) -> Minimap {
    match contextual {
        Minimap::Detecting => update_detecting_context(context, state),
        Minimap::Idle(idle) => {
            update_idle_context(context, state, idle).unwrap_or(Minimap::Detecting)
        }
    }
}

fn update_detecting_context(context: &Context, state: &mut MinimapState) -> Minimap {
    let Update::Ok((anchors, bbox)) =
        update_detection_task(context, 2000, &mut state.minimap_task, move |detector| {
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
        rune_fail_count: 0,
        has_elite_boss: false,
        has_guildie_player: Threshold::new(2),
        has_stranger_player: Threshold::new(2),
        has_friend_player: Threshold::new(2),
        portals: Array::new(),
        platforms,
        platforms_bound,
    })
}

fn update_idle_context(
    context: &Context,
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
        rune_fail_count,
        has_elite_boss,
        has_guildie_player,
        has_stranger_player,
        has_friend_player,
        portals,
        mut platforms,
        mut platforms_bound,
        ..
    } = idle;
    let tl_pixel = pixel_at(context.detector_unwrap().mat(), anchors.tl.0)?;
    let br_pixel = pixel_at(context.detector_unwrap().mat(), anchors.br.0)?;
    let tl_match = anchor_match(anchors.tl.1, tl_pixel);
    let br_match = anchor_match(anchors.br.1, br_pixel);
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
    let (rune, rune_fail_count) =
        update_rune_task(context, &mut state.rune_task, bbox, rune, rune_fail_count);
    let has_elite_boss =
        update_elite_boss_task(context, &mut state.has_elite_boss_task, has_elite_boss);
    let has_guildie_player = update_other_player_task(
        context,
        &mut state.has_guildie_player_task,
        bbox,
        has_guildie_player,
        OtherPlayerKind::Guildie,
    );
    let has_stranger_player = update_other_player_task(
        context,
        &mut state.has_stranger_player_task,
        bbox,
        has_stranger_player,
        OtherPlayerKind::Stranger,
    );
    let has_friend_player = update_other_player_task(
        context,
        &mut state.has_friend_player_task,
        bbox,
        has_friend_player,
        OtherPlayerKind::Friend,
    );
    let portals = update_portals_task(context, &mut state.portals_task, portals, bbox);

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
        rune_fail_count,
        has_elite_boss,
        has_guildie_player,
        has_stranger_player,
        has_friend_player,
        portals,
        platforms,
        platforms_bound,
        ..idle
    }))
}

#[inline]
fn anchor_match(anchor: Vec4b, pixel: Vec4b) -> bool {
    const ANCHOR_ACCEPTABLE_ERROR_RANGE: u32 = 45;

    let b = anchor[0].abs_diff(pixel[0]) as u32;
    let g = anchor[1].abs_diff(pixel[1]) as u32;
    let r = anchor[2].abs_diff(pixel[2]) as u32;
    let avg = (b + g + r) / 3; // Average for grayscale
    avg <= ANCHOR_ACCEPTABLE_ERROR_RANGE
}

#[inline]
fn update_rune_task(
    context: &Context,
    task: &mut Option<Task<Result<Point>>>,
    minimap: Rect,
    rune: Option<Point>,
    rune_fail_count: u32,
) -> (Option<Point>, u32) {
    const MAX_RUNE_FAIL_COUNT: u32 = 3;

    let was_none = rune.is_none();
    let update = if matches!(context.player, Player::SolvingRune(_)) && rune.is_some() {
        Update::Pending
    } else {
        update_detection_task(context, 10000, task, move |detector| {
            detector
                .detect_minimap_rune(minimap)
                .map(|rune| center_of_bbox(rune, minimap))
        })
    };
    match update {
        Update::Ok(rune) => {
            if was_none && !context.halting {
                let _ = context
                    .notification
                    .schedule_notification(NotificationKind::RuneAppear);
            }
            (Some(rune), 0)
        }
        Update::Err(_) => {
            if !was_none {
                if rune_fail_count >= MAX_RUNE_FAIL_COUNT {
                    (None, 0)
                } else {
                    (rune, rune_fail_count + 1)
                }
            } else {
                (rune, rune_fail_count)
            }
        }
        Update::Pending => (rune, rune_fail_count),
    }
}

#[inline]
fn update_elite_boss_task(
    context: &Context,
    task: &mut Option<Task<Result<bool>>>,
    has_elite_boss: bool,
) -> bool {
    let update = update_detection_task(context, 10000, task, move |detector| {
        Ok(detector.detect_elite_boss_bar())
    });
    match update {
        Update::Ok(current_has_elite_boss) => {
            if !has_elite_boss && current_has_elite_boss && !context.halting {
                let _ = context
                    .notification
                    .schedule_notification(NotificationKind::EliteBossAppear);
            }
            current_has_elite_boss
        }
        Update::Pending => has_elite_boss,
        Update::Err(_) => unreachable!(),
    }
}

#[inline]
fn update_other_player_task(
    context: &Context,
    task: &mut Option<Task<Result<bool>>>,
    minimap: Rect,
    threshold: Threshold<bool>,
    kind: OtherPlayerKind,
) -> Threshold<bool> {
    let has_player = threshold.value.unwrap_or_default();
    let threshold = update_threshold_detection(context, 5000, threshold, task, move |detector| {
        Ok(detector.detect_player_kind(minimap, kind))
    });
    if !context.halting && !has_player && threshold.value.unwrap_or_default() {
        let notification = match kind {
            OtherPlayerKind::Guildie => NotificationKind::PlayerGuildieAppear,
            OtherPlayerKind::Stranger => NotificationKind::PlayerStrangerAppear,
            OtherPlayerKind::Friend => NotificationKind::PlayerFriendAppear,
        };
        let _ = context.notification.schedule_notification(notification);
    }
    threshold
}

#[inline]
fn update_portals_task(
    context: &Context,
    task: &mut Option<Task<Result<Vec<Rect>>>>,
    portals: Array<Rect, 16>,
    minimap: Rect,
) -> Array<Rect, 16> {
    let update = update_detection_task(context, 5000, task, move |detector| {
        detector.detect_minimap_portals(minimap)
    });
    match update {
        Update::Ok(vec) if portals.len() < vec.len() => {
            Array::from_iter(vec.into_iter().map(|portal| {
                Rect::new(
                    portal.x,
                    minimap.height - portal.y,
                    portal.width,
                    portal.height,
                )
            }))
        }
        Update::Ok(_) | Update::Err(_) | Update::Pending => portals,
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
fn update_threshold_detection<T, F>(
    context: &Context,
    repeat_delay_millis: u64,
    mut threshold: Threshold<T>,
    threshold_task: &mut Option<Task<Result<T>>>,
    threshold_task_fn: F,
) -> Threshold<T>
where
    T: fmt::Debug + Send + 'static,
    F: FnOnce(Box<dyn Detector>) -> Result<T> + Send + 'static,
{
    let update = update_detection_task(
        context,
        repeat_delay_millis,
        threshold_task,
        threshold_task_fn,
    );

    match update {
        Update::Ok(value) => {
            threshold.value = Some(value);
        }
        Update::Err(_) => {
            if threshold.value.is_some() {
                if threshold.fail_count >= threshold.max_fail_count {
                    threshold.value = None;
                    threshold.fail_count = 0;
                } else {
                    threshold.fail_count += 1;
                }
            }
        }
        Update::Pending => (),
    }

    threshold
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
        detector: MockDetector,
        state: &mut MinimapState,
    ) -> Minimap {
        let context = Context::new(None, Some(detector));
        let completed = |state: &MinimapState| {
            if matches!(contextual, Minimap::Idle(_)) {
                state.rune_task.as_ref().unwrap().completed()
            } else {
                state.minimap_task.as_ref().unwrap().completed()
            }
        };
        let mut minimap = update_context(contextual, &context, state);
        while !completed(state) {
            minimap = update_context(minimap, &context, state);
            time::advance(Duration::from_millis(1000)).await;
        }
        minimap
    }

    #[tokio::test(start_paused = true)]
    async fn minimap_detecting_to_idle() {
        let mut state = MinimapState::default();
        let (detector, bbox, anchors, _) = create_mock_detector();

        let minimap = advance_task(Minimap::Detecting, detector, &mut state).await;
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
            rune_fail_count: 0,
            has_elite_boss: false,
            has_guildie_player: Threshold::default(),
            has_stranger_player: Threshold::default(),
            has_friend_player: Threshold::default(),
            portals: Array::new(),
            platforms: Array::new(),
            platforms_bound: None,
        };

        let minimap = advance_task(Minimap::Idle(idle), detector, &mut state).await;
        assert_matches!(minimap, Minimap::Idle(_));
        match minimap {
            Minimap::Idle(idle) => {
                assert_eq!(idle.rune, Some(center_of_bbox(rune_bbox, bbox)));
            }
            _ => unreachable!(),
        }
    }
}
