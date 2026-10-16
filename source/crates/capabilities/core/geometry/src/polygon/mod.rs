use std::cmp::Ordering;

use campfire_math::{Flat, Num, Vec3};
use campfire_sim::Position;

use crate::halves::Halves;
use crate::polygon::error::PolygonError;

pub(crate) mod error;

/// A simple polygon on the ground plane: at least three points `[x, z]`, its edges from each
/// point to the next and from the last to the first, none meeting another but where two that
/// follow each other share their point. A wall's or a brush's area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polygon {
    points: Vec<[Num; 2]>,
}

impl Polygon {
    /// The polygon of `points`; an error for fewer than three, for a point past the world's
    /// bound, beyond which its edges' products leave a `Flat`'s range, or for two edges that meet
    /// where they should not: two that do not follow each other meet anywhere, or two that do
    /// overlap beyond the point they share, as a zero-length edge or a turn straight back does.
    pub fn new(points: impl IntoIterator<Item = [Num; 2]>) -> Result<Self, PolygonError> {
        Self::of(points.into_iter().collect())
    }

    fn of(points: Vec<[Num; 2]>) -> Result<Self, PolygonError> {
        let count = points.len();
        if count < 3 {
            return Err(PolygonError::TooFewPoints);
        }
        let past = |&[x, z]: &[Num; 2]| !Position::from(Vec3::new(x, Num::ZERO, z)).within_bound();
        if let Some(point) = points.iter().position(past) {
            return Err(PolygonError::PastBound { point });
        }
        let polygon = Self { points };
        for first in 0..count {
            for second in first + 1..count {
                if polygon.edges_meet(first, second) {
                    return Err(PolygonError::EdgesMeet { first, second });
                }
            }
        }
        Ok(polygon)
    }

    /// Whether the point `at`, in halves of a bit, lies inside the polygon or on its edge,
    /// exactly: on an edge when it is on the edge's line between its ends; inside when a ray
    /// from it along +x crosses the edges an odd number of times, each edge counted once over
    /// the half-open span of its z.
    pub(crate) fn holds(&self, at: Flat) -> bool {
        let mut inside = false;
        for edge in 0..self.points.len() {
            let [a, b] = self.edge(edge);
            if on_segment(a, b, at) {
                return true;
            }
            if (a[1] > at[1]) != (b[1] > at[1]) {
                // Negative when `at` lies left of the edge from `a` to `b`.
                let side = (at - a).cross(b - a);
                if (b[1] > a[1] && side < 0) || (b[1] < a[1] && side > 0) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Whether `at`, `[x, z]`, lies inside the polygon or on its edge.
    pub(crate) fn holds_point(&self, at: [Num; 2]) -> bool {
        self.holds(Flat::from_array(at.map(Halves::of)))
    }

    pub fn points(&self) -> &[[Num; 2]] {
        &self.points
    }

    /// The edge from point `at` to the next, in halves of a bit.
    fn edge(&self, at: usize) -> [Flat; 2] {
        let next = (at + 1) % self.points.len();
        [self.points[at], self.points[next]].map(|[x, z]| Flat::new(Halves::of(x), Halves::of(z)))
    }

    /// Whether edges `first` and `second`, `first` the lower, meet where a simple polygon's do
    /// not.
    fn edges_meet(&self, first: usize, second: usize) -> bool {
        let count = self.points.len();
        let [a, b] = self.edge(first);
        let [c, d] = self.edge(second);
        if second == first + 1 {
            return overlaps_on(a, b, d);
        }
        if first == 0 && second == count - 1 {
            return overlaps_on(c, d, b);
        }
        segments_meet([a, b], [c, d])
    }
}

/// The side of the line from `a` to `b` that `c` lies on: `Greater` to the left.
fn orient(a: Flat, b: Flat, c: Flat) -> Ordering {
    (b - a).cross(c - a).cmp(&0)
}

/// Whether `c` lies on the segment from `a` to `b`, its ends included.
fn on_segment(a: Flat, b: Flat, c: Flat) -> bool {
    orient(a, b, c) == Ordering::Equal
        && (0..2).all(|axis| a[axis].min(b[axis]) <= c[axis] && c[axis] <= a[axis].max(b[axis]))
}

/// Whether the edge from `a` to `b` and the one from `b` to `c` share more than `b`: one of no
/// length, or one that turns straight back along the other.
fn overlaps_on(a: Flat, b: Flat, c: Flat) -> bool {
    orient(a, b, c) == Ordering::Equal && (b - a).dot(c - b) <= 0
}

/// Whether two closed segments share a point: each one's ends lie on either side of the
/// other's line, or an end of one lies on the other.
fn segments_meet([a, b]: [Flat; 2], [c, d]: [Flat; 2]) -> bool {
    let crosses = orient(a, b, c) != orient(a, b, d) && orient(c, d, a) != orient(c, d, b);
    crosses
        || on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
}

#[cfg(test)]
mod tests;
