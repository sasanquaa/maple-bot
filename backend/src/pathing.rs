use core::range::Range;
use std::{
    cmp::{Reverse, max, min},
    collections::{BinaryHeap, HashMap},
};

use opencv::core::{Point, Rect};

use crate::array::Array;

pub const MAX_PLATFORMS_COUNT: usize = 24;

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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PlatformWithNeighbors {
    inner: Platform,
    neighbors: Array<Platform, MAX_PLATFORMS_COUNT>,
}

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

pub fn find_platforms_bound(minimap: Rect, platforms: &[PlatformWithNeighbors]) -> Option<Rect> {
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
    platforms: &[PlatformWithNeighbors],
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
    let from_platform = find_platform(&platforms, from, jump_threshold)?;
    let to_platform = find_platform(&platforms, to, jump_threshold)?;
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
    jump_threshold: i32,
) -> Option<Platform> {
    platforms
        .keys()
        .filter(|platform| platform.xs.contains(&point.x))
        .min_by_key(|platform| (platform.y - point.y).abs())
        .filter(|platform| (platform.y - point.y).abs() <= jump_threshold)
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
        !(first.is_empty()
            || second.is_empty()
            || first.end < second.start
            || first.start >= second.end
            || second.end < first.start
            || second.start >= first.end)
    }
    inner(first.into(), second.into())
}

// TODO: more unit tests
#[cfg(test)]
mod tests {
    use crate::pathing::ranges_overlap;

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
}
