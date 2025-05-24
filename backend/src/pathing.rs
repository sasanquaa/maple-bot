use core::range::Range;
use std::{
    cmp::{Reverse, max, min},
    collections::{BinaryHeap, HashMap},
};

use opencv::core::{Point, Rect};

use crate::array::Array;

pub const MAX_PLATFORMS_COUNT: usize = 24;

/// The kind of movement the player should perform
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum MovementHint {
    /// Infers the movement needed
    Infer,
    /// Performs a walk and then jump
    WalkAndJump,
}

/// A platform where player can stand on
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Platform {
    xs: Range<i32>,
    y: i32,
}

impl Platform {
    pub fn new<R: Into<Range<i32>>>(xs: R, y: i32) -> Self {
        Self { xs: xs.into(), y }
    }
}

/// A platform along with its reachable neighbor platforms
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PlatformWithNeighbors {
    inner: Platform,
    neighbors: Array<Platform, MAX_PLATFORMS_COUNT>,
}

impl PlatformWithNeighbors {
    #[inline]
    pub fn xs(&self) -> Range<i32> {
        self.inner.xs
    }

    #[inline]
    pub fn y(&self) -> i32 {
        self.inner.y
    }
}

/// The platform being visited during path finding
#[derive(PartialEq, Eq)]
struct VisitingPlatform {
    score: u32,
    platform: Platform,
}

impl PartialOrd for VisitingPlatform {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VisitingPlatform {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.cmp(&other.score)
    }
}

/// Finds the smallest bounding rectangle that contains all given platforms.
///
/// Returns [`None`] if the list of platforms is empty
pub fn find_platforms_bound(
    minimap: Rect,
    platforms: &Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT>,
) -> Option<Rect> {
    platforms
        .iter()
        .map(|platform| {
            Rect::new(
                platform.inner.xs.start,
                minimap.height - platform.inner.y,
                platform.inner.xs.end - platform.inner.xs.start,
                1,
            )
        })
        .reduce(|acc, cur| acc | cur)
        .map(|bound| {
            // Increase top edge
            Rect::new(bound.x, bound.y - 3, bound.width, bound.height + 3)
        })
}

/// Builds a list of `PlatformWithNeighbors` from  `&[Platforms]` by determining which platforms
/// are reachable from each other.
///
/// The following thresholds are used to determine reachability:
/// - `double_jump_threshold`: minimum x distance required for a double jump
/// - `jump_threshold`: minimum y distance required for a regular jump
/// - `grappling_threshold`: maximum allowed y vertical distance to grapple upward
pub fn find_neighbors(
    platforms: &[Platform],
    double_jump_threshold: i32,
    jump_threshold: i32,
    grappling_threshold: i32,
) -> Vec<PlatformWithNeighbors> {
    let mut vec = Vec::with_capacity(platforms.len());
    for i in 0..platforms.len() {
        let current = platforms[i];
        let mut neighbors = Array::new();
        for j in (0..i).chain(i + 1..platforms.len()) {
            if platforms_reachable(
                current,
                platforms[j],
                double_jump_threshold,
                jump_threshold,
                grappling_threshold,
            ) {
                neighbors.push(platforms[j]);
            }
        }
        vec.push(PlatformWithNeighbors {
            inner: current,
            neighbors,
        });
    }
    vec
}

/// Finds a sequence of points representing a path from `from` to `to`, using the given
/// platform map.
///
/// `vertical_threshold` represents maximum y distance between two connected platforms to perform
/// a grappling. This is used as weight score to help prioritize vertical movement over
/// horizontal movement. If `enable_hint` is true, provides movement hints like `WalkAndJump`.
pub fn find_points_with(
    platforms: &Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT>,
    from: Point,
    to: Point,
    enable_hint: bool,
    double_jump_threshold: i32,
    jump_threshold: i32,
    vertical_threshold: i32,
) -> Option<Vec<(Point, MovementHint)>> {
    let platforms = platforms
        .iter()
        .map(|platform| (platform.inner, *platform))
        .collect::<HashMap<_, _>>();
    let from_platform = find_platform(&platforms, from, None)?; // Clamp `from` to nearest platform
    let to_platform = find_platform(&platforms, to, Some(jump_threshold))?;
    let mut came_from = HashMap::<Platform, Platform>::new();
    let mut visiting = BinaryHeap::new();
    let mut score = HashMap::<Platform, u32>::new();

    visiting.push(Reverse(VisitingPlatform {
        score: 0,
        platform: from_platform,
    }));
    score.insert(from_platform, 0);

    while !visiting.is_empty() {
        let current = visiting.pop().unwrap().0;
        let current_score = score.get(&current.platform).copied().unwrap_or(u32::MAX);
        if current.platform == to_platform {
            return points_from(
                &came_from,
                from,
                from_platform,
                to_platform,
                to,
                enable_hint,
                double_jump_threshold,
                jump_threshold,
            );
        }

        let neighbors = platforms[&current.platform].neighbors;
        for neighbor in neighbors {
            let tentative_score = current_score.saturating_add(weight_score(
                current.platform,
                neighbor,
                vertical_threshold,
            ));
            let neighbor_score = score.get(&neighbor).copied().unwrap_or(u32::MAX);
            if tentative_score < neighbor_score {
                came_from.insert(neighbor, current.platform);
                score.insert(neighbor, tentative_score);
                if !visiting
                    .iter()
                    .any(|platform| platform.0.platform == neighbor)
                {
                    visiting.push(Reverse(VisitingPlatform {
                        score: tentative_score,
                        platform: neighbor,
                    }));
                }
            }
        }
    }
    None
}

/// Converts a path from the `came_from` graph into a list of `(Point, MovementHint)` pairs
/// indicating how to move from `from` to `to`.
///
/// Adds offsets to handle jump and landing safety margins.
#[allow(clippy::too_many_arguments)]
fn points_from(
    came_from: &HashMap<Platform, Platform>,
    from: Point,
    from_platform: Platform,
    to_platform: Platform,
    to: Point,
    enable_hint: bool,
    double_jump_threshold: i32,
    jump_threshold: i32,
) -> Option<Vec<(Point, MovementHint)>> {
    /// A margin of error to ensure double jump slide on landing does not make the
    /// player drops from platform
    const DOUBLE_JUMP_EXTRA_OFFSET: i32 = 10;

    /// A margin of error to ensure jump is launched just before the platform edge
    const JUMP_OFFSET: i32 = 2;

    const WALK_AND_JUMP_THRESHOLD: i32 = 12;

    let mut current = to_platform;
    let mut went_to = HashMap::new();
    while came_from.contains_key(&current) {
        let next = came_from[&current];
        went_to.insert(next, current);
        current = next;
    }
    current = from_platform;

    let mut points = vec![];
    let mut last_point = Point::new(from.x, current.y);
    let double_jump_offset = double_jump_threshold / 2 + DOUBLE_JUMP_EXTRA_OFFSET;
    while went_to.contains_key(&current) {
        let next = went_to[&current];
        let start_max = max(next.xs.start, current.xs.start);
        let end_min = min(next.xs.end, current.xs.end);

        // Check if the current platform overlap with the next platform
        if ranges_overlap(next.xs, current.xs) {
            if (start_max..end_min).contains(&last_point.x) {
                // Already inside intersection range, add a point to move up or down
                points.push((Point::new(last_point.x, next.y), MovementHint::Infer));
            } else {
                // Outside intersection range, add 2 points to move inside and then up or down
                let x = rand::random_range(start_max..end_min);
                points.push((Point::new(x, current.y), MovementHint::Infer));
                points.push((Point::new(x, next.y), MovementHint::Infer));
            }
        } else {
            let is_ltr = current.xs.start < next.xs.start;
            // Check if can double jump from last_point
            let can_double_jump_last_point = if is_ltr {
                current.xs.end - last_point.x > double_jump_threshold
            } else {
                last_point.x - current.xs.start + 1 > double_jump_threshold
            };
            // Ignore initial point as it has the same platform as the current
            let can_double_jump_last_point = can_double_jump_last_point && points.len() > 1;

            // Check if the two platforms are close enough to just do a walk and jump
            let (offset, hint) = if enable_hint
                && !can_double_jump_last_point
                && start_max - end_min <= WALK_AND_JUMP_THRESHOLD
                && (current.y - next.y).abs() <= jump_threshold
            {
                (JUMP_OFFSET, MovementHint::WalkAndJump)
            } else {
                (double_jump_offset, MovementHint::Infer)
            };

            let from_edge = if is_ltr {
                (current.xs.end - 1 - offset).clamp(current.xs.start, current.xs.end - 1)
            } else {
                (current.xs.start + offset).clamp(current.xs.start, current.xs.end - 1)
            };
            let from_point = Point::new(from_edge, current.y);
            points.push((from_point, hint));
        }

        last_point = points.last().copied().unwrap().0;
        current = next;
    }

    points.push((Point::new(to.x, to_platform.y), MovementHint::Infer));

    Some(points)
}

/// Finds the closest platform underneath or near a given `point`.
///
/// If `jump_threshold` is provided, it limits how far vertically the point can be from a platform.
#[inline]
fn find_platform(
    platforms: &HashMap<Platform, PlatformWithNeighbors>,
    point: Point,
    jump_threshold: Option<i32>,
) -> Option<Platform> {
    platforms
        .keys()
        .filter(|platform| platform.xs.contains(&point.x))
        .min_by_key(|platform| (platform.y - point.y).abs())
        .filter(|platform| {
            jump_threshold.is_none() || (platform.y - point.y).abs() <= jump_threshold.unwrap()
        })
        .copied()
}

#[inline]
fn weight_score(current: Platform, neighbor: Platform, vertical_threshold: i32) -> u32 {
    let y_distance = (current.y - neighbor.y).abs();
    if y_distance <= vertical_threshold {
        y_distance as u32
    } else {
        u32::MAX
    }
}

/// Determines whether the two platforms are reachable from one another.
///
/// One platform is reachable to another platform if:
/// - The two platforms [`Platform::xs`] overlap and one is above the other or can be grappled to
/// - The two platforms [`Platform::xs`] do not overlap but can double jump from one to another
#[inline]
fn platforms_reachable(
    from: Platform,
    to: Platform,
    double_jump_threshold: i32,
    jump_threshold: i32,
    grappling_threshold: i32,
) -> bool {
    let diff = from.y - to.y;
    if !ranges_overlap(from.xs, to.xs) {
        if diff >= 0 || diff.abs() <= jump_threshold {
            return max(from.xs.start, to.xs.start) - min(from.xs.end, to.xs.end)
                <= double_jump_threshold;
        }
        return false;
    }
    if from.xs.is_empty() || to.xs.is_empty() {
        return false;
    }
    diff >= 0 || diff.abs() <= grappling_threshold
}

#[inline]
fn ranges_overlap<R: Into<Range<i32>>>(first: R, second: R) -> bool {
    fn inner(first: Range<i32>, second: Range<i32>) -> bool {
        !first.is_empty()
            && !second.is_empty()
            && ((first.start < second.end && first.start >= second.start)
                || (second.start < first.end && second.start >= first.start))
    }
    inner(first.into(), second.into())
}

#[cfg(test)]
mod tests {
    use opencv::core::Point;

    use super::{
        MAX_PLATFORMS_COUNT, MovementHint, Platform, PlatformWithNeighbors, find_neighbors,
    };
    use crate::{
        array::Array,
        pathing::{find_points_with, ranges_overlap},
    };

    fn make_platforms_with_neighbors(
        platforms: &[Platform],
    ) -> Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT> {
        let connected = find_neighbors(platforms, 25, 7, 41);
        let mut array = Array::new();
        for p in connected {
            array.push(p);
        }
        array
    }

    #[test]
    fn ranges_xs_overlap_cases() {
        assert!(ranges_overlap(3..6, 4..7));
        assert!(ranges_overlap(4..7, 3..6));
        assert!(ranges_overlap(444..555, 445..446));
        assert!(!ranges_overlap(4..4, 4..5));
        assert!(!ranges_overlap(100..1000, 55..66));
        assert!(!ranges_overlap(3..i32::MAX, 1..3));
        assert!(!ranges_overlap(5..10, 0..5));
    }

    #[test]
    fn find_points_with_direct_overlap() {
        let platforms = [
            Platform::new(0..100, 50),
            Platform::new(0..100, 60), // Directly above
        ];
        let platforms = make_platforms_with_neighbors(&platforms);

        let from = Point::new(10, 50);
        let to = Point::new(20, 60);

        let points = find_points_with(&platforms, from, to, true, 25, 7, 41).unwrap();

        let expected = vec![
            (Point::new(10, 60), MovementHint::Infer),
            (Point::new(20, 60), MovementHint::Infer),
        ];

        assert_eq!(points, expected);
    }

    #[test]
    fn find_points_with_non_overlapping_jump() {
        let platforms = [
            Platform::new(0..50, 50),
            Platform::new(60..110, 55), // Reachable by double jump
        ];
        let platforms = make_platforms_with_neighbors(&platforms);

        let from = Point::new(25, 50);
        let to = Point::new(65, 55);

        let points = find_points_with(&platforms, from, to, true, 25, 7, 41).unwrap();

        assert_eq!(points.first().unwrap().0.y, 50);
        assert_eq!(points.last().unwrap().0.y, 55);
        assert!(points.len() >= 2);
    }

    #[test]
    fn find_points_with_multi_hop_path() {
        let platforms = [
            Platform::new(0..50, 50),
            Platform::new(0..50, 91),
            Platform::new(0..50, 132),
        ];
        let platforms = make_platforms_with_neighbors(&platforms);

        let from = Point::new(10, 50);
        let to = Point::new(20, 132);

        let points = find_points_with(&platforms, from, to, true, 25, 7, 41).unwrap();

        // Check that y-values ascend (multi-hop upward movement)
        let ys: Vec<_> = points.iter().map(|(p, _)| p.y).collect();
        assert!(
            ys.windows(2).all(|w| w[0] <= w[1]),
            "Expected ascending y values in multi-hop: {ys:?}",
        );

        assert_eq!(points.first().unwrap().0.y, 91);
        assert_eq!(points.last().unwrap().0.y, 132);
    }

    #[test]
    fn find_points_with_no_path() {
        let platforms = [
            Platform::new(0..50, 50),
            Platform::new(100..150, 55), // Too far
        ];
        let platforms = make_platforms_with_neighbors(&platforms);

        let from = Point::new(25, 50);
        let to = Point::new(125, 55);

        let points = find_points_with(&platforms, from, to, true, 25, 7, 41);
        assert!(points.is_none());
    }

    #[test]
    fn find_points_with_walk_and_jump_hint() {
        let platforms = [
            Platform::new(0..50, 50),
            Platform::new(55..61, 52), // Only 5 units of horizontal gap
        ];
        let platforms = make_platforms_with_neighbors(&platforms);

        let from = Point::new(45, 50); // Near right edge of first platform
        let to = Point::new(60, 52); // Near left edge of second platform

        let points = find_points_with(&platforms, from, to, true, 25, 7, 41).unwrap();

        let has_walk_and_jump = points
            .iter()
            .any(|(_, hint)| *hint == MovementHint::WalkAndJump);
        assert!(
            has_walk_and_jump,
            "Expected at least one WalkAndJump movement hint, got: {points:?}",
        );

        assert_eq!(points.first().unwrap().0.y, 50);
        assert_eq!(points.last().unwrap().0.y, 52);
    }
}
