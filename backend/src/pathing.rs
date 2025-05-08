use core::range::Range;
use std::{
    cmp::{Reverse, max, min},
    collections::{BinaryHeap, HashMap},
};

use opencv::core::{Point, Rect};

use crate::array::Array;

pub const MAX_PLATFORMS_COUNT: usize = 24;

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

/// Platforms connected togehter into neighbors
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

/// Finds a rectangular bound that contains all the provided platforms
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

/// Connects platforms into neighbors from the provided `platforms`
///
/// One platform is connected to another platform if:
/// - The two platforms [`Platform::xs`] overlap and one is above the other or can be grappled to
/// - The two platforms [`Platform::xs`] do not overlap but can double jump from one to another
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

pub fn find_points_with(
    platforms: &Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT>,
    from: Point,
    to: Point,
    double_jump_threshold: i32,
    jump_threshold: i32,
    vertical_threshold: i32,
) -> Option<Vec<Point>> {
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
                double_jump_threshold,
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

fn points_from(
    came_from: &HashMap<Platform, Platform>,
    from: Point,
    from_platform: Platform,
    to_platform: Platform,
    to: Point,
    double_jump_threshold: i32,
) -> Option<Vec<Point>> {
    // a margin of error to ensure double jump is launched
    const DOUBLE_JUMP_THRESHOLD_OFFSET: i32 = 7;

    // TODO: too complex maybe?
    let mut current = to_platform;
    let mut went_to = HashMap::new();
    while came_from.contains_key(&current) {
        let next = came_from[&current];
        went_to.insert(next, current);
        current = next;
    }

    current = from_platform;
    let mut points = vec![Point::new(from.x, current.y)];
    let mut last_point = points.last().copied().unwrap();
    while went_to.contains_key(&current) {
        let next = went_to[&current];
        if ranges_overlap(next.xs, current.xs) {
            let start_max = max(next.xs.start, current.xs.start);
            let end_min = min(next.xs.end, current.xs.end);
            if (start_max..end_min).contains(&last_point.x) {
                points.push(Point::new(last_point.x, next.y));
            } else {
                let x = rand::random_range(start_max..end_min);
                points.push(Point::new(x, current.y));
                points.push(Point::new(x, next.y));
            }
        } else {
            let start_max = max(next.xs.start, current.xs.start);
            let end_min = min(next.xs.end, current.xs.end);
            let length = max(double_jump_threshold - (start_max - end_min), 0)
                + DOUBLE_JUMP_THRESHOLD_OFFSET;
            // TODO: "soft" double jump
            if start_max == current.xs.start {
                // right to left
                let mut start_max_offset = last_point.x;
                while start_max_offset - length > start_max {
                    start_max_offset -= length;
                    points.push(Point::new(start_max_offset, current.y));
                }
                let end_min_offset = length - (start_max_offset - start_max);
                let end_min_offset = max(end_min - 1 - end_min_offset, next.xs.start);
                debug_assert!(start_max_offset >= start_max && start_max_offset < current.xs.end);
                debug_assert!(end_min_offset < end_min && end_min_offset >= next.xs.start);
                points.push(Point::new(start_max_offset, current.y));
                points.push(Point::new(end_min_offset, next.y));
            } else {
                // left to right
                let mut end_min_offset = last_point.x;
                while end_min_offset + length < end_min {
                    end_min_offset += length;
                    points.push(Point::new(end_min_offset, current.y));
                }
                let start_max_offset = length - (end_min - 1 - end_min_offset);
                let start_max_offset = min(start_max + start_max_offset, next.xs.end - 1);
                debug_assert!(start_max_offset >= start_max && start_max_offset < next.xs.end);
                debug_assert!(end_min_offset < end_min && end_min_offset >= current.xs.start);
                points.push(Point::new(end_min_offset, current.y));
                points.push(Point::new(start_max_offset, next.y));
            }
        }
        last_point = points.last().copied().unwrap();
        current = next;
    }
    points.push(Point::new(to.x, to_platform.y));
    Some(points)
}

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
    if y_distance < vertical_threshold {
        y_distance as u32
    } else {
        u32::MAX
    }
}

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

    use super::{MAX_PLATFORMS_COUNT, Platform, PlatformWithNeighbors, find_neighbors};
    use crate::{
        array::Array,
        pathing::{find_points_with, ranges_overlap},
    };

    fn make_platforms_with_neighbors(
        platforms: &[Platform],
    ) -> Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT> {
        let connected = find_neighbors(platforms, 20, 15, 30);
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

        let points = find_points_with(&platforms, from, to, 20, 15, 50).unwrap();

        assert_eq!(points.len(), 3);
        assert_eq!(
            points,
            vec![Point::new(10, 50), Point::new(10, 60), Point::new(20, 60),]
        );
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

        let points = find_points_with(&platforms, from, to, 30, 15, 50).unwrap();

        assert_eq!(points.first().unwrap().y, 50);
        assert_eq!(points.last().unwrap().y, 55);
        assert!(points.len() >= 2); // Should require at least one jump point
    }

    #[test]
    fn find_points_with_multi_hop_path() {
        let platforms = [
            Platform::new(0..50, 50),
            Platform::new(0..50, 60),
            Platform::new(0..50, 70),
        ];
        let platforms = make_platforms_with_neighbors(&platforms);

        let from = Point::new(10, 50);
        let to = Point::new(20, 70);

        let points = find_points_with(&platforms, from, to, 20, 15, 20).unwrap();
        // Expected path goes from 50 → 60 → 70 on the same x (10)
        let expected = vec![
            Point::new(10, 50),
            Point::new(10, 60),
            Point::new(10, 70),
            Point::new(20, 70), // Final destination point, as per function spec
        ];

        assert_eq!(points.len(), expected.len());
        assert_eq!(points, expected);
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

        let points = find_points_with(&platforms, from, to, 20, 15, 20);
        assert!(points.is_none());
    }
}
