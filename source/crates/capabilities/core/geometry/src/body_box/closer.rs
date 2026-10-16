//! A box's test of the points closer than a reach, and its half edges' lines' bounds and
//! directions, made once or on demand.

use campfire_math::Direction;

use crate::body_box::Frame;
use crate::body_box::edges::{Edges, LineBounds, Lines, Reach, Row};

/// A box's test of the points closer than a reach to it, as `Frame::nearest_to` measures them,
/// with what each point's test shares made once: its edges' terms, the reach, and the bounds and
/// directions of the half edges' lines, which edges 0 and 2 and edges 1 and 3 run along. Made on
/// demand, the directions measured 26 % slower, as a cell's interior mutability made each test
/// load the whole test again.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Closer {
    edges: Edges,
    reach: Reach,
    bounds: [LineBounds; 2],
    directions: [Direction<2>; 2],
}

/// A half edge's line's bounds and direction, each made when a test first takes it: for one
/// point's or one path's test, which most often takes neither.
#[derive(Debug, Clone, Copy)]
pub(super) struct LinesOnDemand {
    squares: [i128; 2],
    reach: Reach,
    bounds: [Option<LineBounds>; 2],
    pub(super) directions: HalfDirections,
}

/// The directions of a box's half edges, each made when a test first takes it.
#[derive(Debug, Clone, Copy)]
pub(super) struct HalfDirections {
    frame: Frame,
    made: [Option<Direction<2>>; 2],
}

impl Closer {
    /// The test of the points closer than `reach` units to `frame`.
    pub(super) fn new(frame: Frame, reach: i128) -> Self {
        let edges = Edges::of(frame);
        Self {
            edges,
            reach: Reach::of(reach),
            bounds: [0, 1].map(|half| LineBounds::of(edges.half_squares[half], reach)),
            directions: frame.directions(),
        }
    }

    /// The terms the points of the row at `z` share.
    pub(crate) fn row(&self, z: i64) -> Row {
        self.edges.row(z)
    }

    /// Whether the point at `x` of `row` lies closer than the reach to the box, as
    /// `Edges::closer` decides.
    pub(crate) fn holds(&self, row: &Row, x: i64) -> bool {
        let slabs = self.edges.slabs(row, x);
        if !self.edges.outside(slabs) {
            return self.reach.units > 0;
        }
        // A slab value past the area and a line's bound puts the point at least two units past
        // the reach from the edges along the other half edge, so from the box, with no edge's
        // test: as most cells next to a run lie. Slab 0 crosses the half edge `b`, so it measures
        // from edges 1 and 3, and slab 1 from edges 0 and 2.
        let area = self.edges.area.cast_unsigned();
        let [first, second] = slabs.map(i128::unsigned_abs);
        if first >= area.wrapping_add(self.bounds[1].far)
            || second >= area.wrapping_add(self.bounds[0].far)
        {
            return false;
        }
        let mut lines = self;
        self.edges
            .closer_probe(&self.edges.probe_of(row, x, slabs), &mut lines)
    }
}

impl Lines for &Closer {
    fn reach(&self) -> Reach {
        self.reach
    }

    fn bounds(&mut self, half: usize) -> LineBounds {
        self.bounds[half]
    }

    fn direction(&mut self, half: usize) -> Direction<2> {
        self.directions[half]
    }
}

impl LinesOnDemand {
    pub(super) const fn new(edges: &Edges, reach: Reach) -> Self {
        Self {
            squares: edges.half_squares,
            reach,
            bounds: [None; 2],
            directions: HalfDirections::new(edges.frame),
        }
    }
}

impl Lines for LinesOnDemand {
    fn reach(&self) -> Reach {
        self.reach
    }

    fn bounds(&mut self, half: usize) -> LineBounds {
        let (square, reach) = (self.squares[half], self.reach.units);
        *self.bounds[half].get_or_insert_with(|| LineBounds::of(square, reach))
    }

    fn direction(&mut self, half: usize) -> Direction<2> {
        self.directions.of_half(half)
    }
}

impl HalfDirections {
    pub(super) const fn new(frame: Frame) -> Self {
        Self {
            frame,
            made: [None; 2],
        }
    }

    /// The direction of half edge `half`, 0 or 1.
    pub(super) fn of_half(&mut self, half: usize) -> Direction<2> {
        let frame = self.frame;
        *self.made[half].get_or_insert_with(|| frame.direction(half))
    }

    /// The direction of the outward normal of `edge`, `[z, −x]` of the edge `[x, z]`: edges 0
    /// and 1 run along twice their half edge and edges 2 and 3 along its negation, and a doubled
    /// vector has the same unit, so it is the half edge's direction turned, and negated for
    /// edges 2 and 3. A push reads only its unit.
    pub(super) fn normal(&mut self, edge: usize) -> Direction<2> {
        let turned = self.of_half(edge % 2).turned();
        if edge < 2 { turned } else { -turned }
    }
}
