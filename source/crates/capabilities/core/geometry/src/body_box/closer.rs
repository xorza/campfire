//! A box's test of the points closer than a reach, and what its tests take of each half edge's
//! line: the line's bounds and direction.

use campfire_math::{Direction, FloorRoot};

use crate::body_box::Frame;
use crate::body_box::edges::{Edges, Row};

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

/// What a box's tests take of a reach and of each half edge's line: the reach, the line's
/// bounds, and its direction, for a point between them.
pub(super) trait Lines {
    fn reach(&self) -> Reach;
    fn bounds(&mut self, half: usize) -> LineBounds;
    fn direction(&mut self, half: usize) -> Direction<2>;
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

/// The bounds past which a point's distance from a half edge's line, as the half edge's direction
/// measures it, to the nearest unit and within 0.51 of the true one (design 09, D9), lies against
/// a reach with no direction. The point's cross product with the half edge, `C`, over the half
/// edge's length `|h|`, is the true distance: `|C| ≥ far`, at least `(reach + 2) |h|`, puts it
/// at least two units past the reach, so its measure too; `|C| < near`, below `(reach − 1) |h|`,
/// more than a unit short of it, so its measure short of it too.
#[derive(Debug, Clone, Copy)]
pub(super) struct LineBounds {
    pub(super) far: u128,
    pub(super) near: u128,
}

/// A reach in units, and the largest squared distance of a point closer than it, −1 when none
/// is: the nearest root of a square is below `r ≥ 1` exactly when the square is at most `r² − r`,
/// as `nearest_root_cmp` orders it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Reach {
    pub(super) units: i128,
    pub(super) square_limit: i128,
}

impl Closer {
    /// The test of the points closer than `reach` units to `frame`.
    pub(super) fn new(frame: Frame, reach: i128) -> Closer {
        let edges = Edges::of(frame);
        Closer {
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
    pub(super) const fn new(edges: &Edges, reach: Reach) -> LinesOnDemand {
        LinesOnDemand {
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
    pub(super) const fn new(frame: Frame) -> HalfDirections {
        HalfDirections {
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

impl LineBounds {
    /// The bounds of a half edge whose squared length is `square`, against `reach`: its length
    /// lies from the floor root to the ceiling one, so `far` takes the ceiling and `near` the
    /// floor. A reach and a half edge within the world's bound keep each below 2¹⁰², so the
    /// wrapping steps skip the checks a release build makes.
    pub(super) fn of(square: i128, reach: i128) -> LineBounds {
        debug_assert!(square > 0 && reach >= 0);
        let square = square.cast_unsigned();
        let root = square.floor_root();
        let ceiling = root.wrapping_add(u128::from(root.wrapping_mul(root) < square));
        let reach = reach.cast_unsigned();
        LineBounds {
            far: reach.wrapping_add(2).wrapping_mul(ceiling),
            near: if reach >= 1 {
                (reach - 1).wrapping_mul(root)
            } else {
                0
            },
        }
    }
}

impl Reach {
    pub(super) const fn of(units: i128) -> Reach {
        debug_assert!(units >= 0);
        Reach {
            units,
            square_limit: if units > 0 { units * units - units } else { -1 },
        }
    }
}
