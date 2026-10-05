use std::cmp::Ordering;

use campfire_math::Num;

use crate::values::grid::Grid;
use crate::values::polygon::error::PolygonError;

pub(crate) mod error;

/// A simple polygon on the ground plane: at least three points `[x, z]`, its edges from each
/// point to the next and from the last to the first, none meeting another but where two that
/// follow each other share their point. A wall's or a brush's area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polygon {
    points: Vec<[Num; 2]>,
}

/// A point in halves of a bit, for exact sums and products of coordinates within twice the world's
/// bound.
type Twice = [i128; 2];

impl Polygon {
    /// The polygon of `points`; an error for fewer than three, or for two edges that meet where
    /// they should not: two that do not follow each other meet anywhere, or two that do overlap
    /// beyond the point they share, as a zero-length edge or a turn straight back does.
    pub(crate) fn new(points: Vec<[Num; 2]>) -> Result<Polygon, PolygonError> {
        let count = points.len();
        if count < 3 {
            return Err(PolygonError::TooFewPoints);
        }
        let polygon = Polygon { points };
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
    pub(crate) fn holds(&self, at: Twice) -> bool {
        let mut inside = false;
        for edge in 0..self.points.len() {
            let [a, b] = self.edge(edge);
            if on_segment(a, b, at) {
                return true;
            }
            if (a[1] > at[1]) != (b[1] > at[1]) {
                let along = (at[0] - a[0]) * (b[1] - a[1]);
                let across = (at[1] - a[1]) * (b[0] - a[0]);
                if (b[1] > a[1] && along < across) || (b[1] < a[1] && along > across) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Calls `mark` with each cell of `grid` whose center the polygon holds, in order; the
    /// polygon lies within the grid's bounds.
    pub(crate) fn cells(&self, grid: &Grid, mut mark: impl FnMut(usize)) {
        let first = self.points[0];
        let (low, high) = self
            .points
            .iter()
            .fold((first, first), |(low, high), point| {
                (
                    [0, 1].map(|axis| low[axis].min(point[axis])),
                    [0, 1].map(|axis| high[axis].max(point[axis])),
                )
            });
        let columns = grid.index(0, low[0])..=grid.index(0, high[0]);
        let rows = grid.index(1, low[1])..=grid.index(1, high[1]);
        for row in rows {
            for column in columns.clone() {
                let cell = row * grid.columns() + column;
                if self.holds(grid.center_twice(cell)) {
                    mark(cell);
                }
            }
        }
    }

    /// The edge from point `at` to the next, in halves of a bit.
    fn edge(&self, at: usize) -> [Twice; 2] {
        let next = (at + 1) % self.points.len();
        [self.points[at], self.points[next]].map(|[x, z]| [twice(x), twice(z)])
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

const fn twice(value: Num) -> i128 {
    2 * value.to_bits() as i128
}

/// The side of the line from `a` to `b` that `c` lies on: `Greater` to the left.
fn orient(a: Twice, b: Twice, c: Twice) -> Ordering {
    let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    cross.cmp(&0)
}

/// Whether `c` lies on the segment from `a` to `b`, its ends included.
fn on_segment(a: Twice, b: Twice, c: Twice) -> bool {
    orient(a, b, c) == Ordering::Equal
        && (0..2).all(|axis| a[axis].min(b[axis]) <= c[axis] && c[axis] <= a[axis].max(b[axis]))
}

/// Whether the edge from `a` to `b` and the one from `b` to `c` share more than `b`: one of no
/// length, or one that turns straight back along the other.
fn overlaps_on(a: Twice, b: Twice, c: Twice) -> bool {
    let dot = (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1]);
    orient(a, b, c) == Ordering::Equal && dot <= 0
}

/// Whether two closed segments share a point: each one's ends lie on either side of the
/// other's line, or an end of one lies on the other.
fn segments_meet([a, b]: [Twice; 2], [c, d]: [Twice; 2]) -> bool {
    let crosses = orient(a, b, c) != orient(a, b, d) && orient(c, d, a) != orient(c, d, b);
    crosses
        || on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
}

#[cfg(test)]
mod tests;
