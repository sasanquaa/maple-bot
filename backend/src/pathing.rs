use core::range::Range;
use std::{
    cmp::{Reverse, max, min},
    collections::{BinaryHeap, HashMap},
};

use opencv::core::Point;

use crate::array::Array;

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
    neighbors: Array<Platform, 16>,
}

#[derive(PartialEq, Eq)]
struct VisitingPlatform {
    score: u32,
    platform: Platform,
}

impl PartialOrd for VisitingPlatform {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.score.partial_cmp(&other.score)
    }
}

impl Ord for VisitingPlatform {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.cmp(&other.score)
    }
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
    find_points(
        platforms,
        from,
        to,
        double_jump_threshold,
        jump_threshold,
        vertical_threshold,
    )
}

fn find_points(
    platforms: &[PlatformWithNeighbors],
    from: Point,
    to: Point,
    double_jump_threshold: i32,
    jump_threshold: i32,
    vertical_threshold: i32,
) -> Option<Vec<Point>> {
    let platforms = platforms
        .into_iter()
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
            return points_from(&came_from, to_platform, to, double_jump_threshold);
        }
        let neighbors = platforms[&current.platform].neighbors.clone();
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
                if visiting
                    .iter()
                    .find(|platform| platform.0.platform == neighbor)
                    .is_none()
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
    to_platform: Platform,
    to: Point,
    double_jump_threshold: i32,
) -> Option<Vec<Point>> {
    const DOUBLE_JUMP_THRESHOLD_OFFSET: i32 = 10;

    let mut current = to_platform;
    let mut points = vec![Point::new(to.x, current.y)];
    while came_from.contains_key(&current) {
        let next = came_from[&current];
        if ranges_overlap(next.xs, current.xs) {
            let start_max = max(next.xs.start, current.xs.start);
            let end_min = min(next.xs.end, current.xs.end);
            let x = (start_max + end_min - 1) / 2;
            points.push(Point::new(x, current.y));
            points.push(Point::new(x, next.y));
        } else {
            let start_max = max(next.xs.start, current.xs.start);
            let end_min = min(next.xs.end, current.xs.end);
            let length = max(double_jump_threshold - (start_max - end_min), 0)
                + DOUBLE_JUMP_THRESHOLD_OFFSET;
            // TODO: bound check pls
            // TODO: "soft" double jump
            if start_max == current.xs.start {
                points.push(Point::new(start_max + length / 2, current.y));
                points.push(Point::new(end_min - length / 2, next.y));
            } else {
                points.push(Point::new(end_min - length / 2, current.y));
                points.push(Point::new(start_max + length / 2, next.y));
            }
        }
        current = next;
    }
    points.reverse();
    return Some(points);
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
    if !ranges_overlap(current.xs, neighbor.xs) {
        (y_distance
            + min(
                (neighbor.xs.start - current.xs.end).abs(),
                (current.xs.start - neighbor.xs.end).abs(),
            )) as u32
    } else {
        if y_distance < vertical_threshold {
            y_distance as u32
        } else {
            u32::MAX
        }
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
        if diff >= 0 || diff.abs() < jump_threshold {
            return min(
                (to.xs.start - from.xs.end).abs(),
                (from.xs.start - to.xs.end).abs(),
            ) < double_jump_threshold;
        }
        return false;
    }
    diff >= 0 || diff.abs() < grappling_threshold
}

#[inline]
fn ranges_overlap<R: Into<Range<i32>>>(first: R, second: R) -> bool {
    fn inner(first: Range<i32>, second: Range<i32>) -> bool {
        if first.is_empty()
            || second.is_empty()
            || first.end < second.start
            || first.start >= second.end
            || second.end < first.start
            || second.start >= first.end
        {
            false
        } else {
            true
        }
    }
    inner(first.into(), second.into())
}

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
