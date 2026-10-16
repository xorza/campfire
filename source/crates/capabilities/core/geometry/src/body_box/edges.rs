//! What each point's measure against a box's edges shares, a point's terms against them, and
//! what the measure takes of a reach and of each half edge's line.

use std::cmp::Ordering;

use campfire_math::{Direction, Flat, FloorRoot, RoundedRoot};

use crate::body_box::{Feature, Frame, Nearest, cross, product};

/// What a box's tests take of a reach and of each half edge's line: the reach, the line's
/// bounds, and its direction, for a point between them.
pub(super) trait Lines {
    fn reach(&self) -> Reach;
    fn bounds(&mut self, half: usize) -> LineBounds;
    fn direction(&mut self, half: usize) -> Direction<2>;
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

/// What each point's measure against a box's edges shares. With `a` and `b` the half edges, the
/// corners are `−a − b`, `a − b`, `a + b` and `b − a`, and the edges from each to the next `2a`,
/// `2b`, `−2a` and `−2b`, so a point's `off · a`, `off · b` and `off · off`, and the box's
/// `a · a`, `a · b` and `b · b`, give each edge's `along`, `(off − start) · edge`, and each
/// corner's squared distance, `|off − corner|²`, exactly. A box and an offset within the world's
/// bound, at twice the scale, keep each term below 2¹⁰⁰, so the wrapping steps skip the checks a
/// release build makes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Edges {
    pub(super) frame: Frame,
    pub(super) area: i128,
    pub(super) corners: [Flat; 4],
    /// `−start · edge`, of edges 0 and 2 and of edges 1 and 3: `2(a · a + a · b)` and
    /// `2(b · b − a · b)`.
    along_from: [i128; 2],
    /// Each edge's squared length, of edges 0 and 2 and of edges 1 and 3: `4 a · a` and `4 b · b`.
    lengths: [i128; 2],
    /// Each corner's squared distance from the position, of corners 0 and 2 and of corners 1 and
    /// 3: `|a + b|²` and `|a − b|²`.
    corner_squares: [i128; 2],
    /// Each half edge's squared length, `a · a` and `b · b`.
    pub(super) half_squares: [i128; 2],
}

/// A point outside a box: its coordinates, its slab values, its products with the box's half
/// edges, and its own square.
#[derive(Debug, Clone, Copy)]
pub(super) struct Probe {
    x: i64,
    z: i64,
    slabs: [i128; 2],
    along_a: i128,
    along_b: i128,
    square: i128,
}

/// What the points of a row, of one `z`, share in their tests against a box: `z`, and the part
/// of `z` in each slab value, each product with a half edge, and the square.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Row {
    z: i64,
    slabs: [i128; 2],
    along: [i128; 2],
    square: i128,
}

/// What of an edge is nearest a point: a corner, by its index, or a point inside the edge, whose
/// distance is the edge's line's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Corner(usize),
    Line,
}

impl Edges {
    pub(super) fn of(frame: Frame) -> Self {
        let [a, b] = frame.halves;
        let dot = |u: Flat, v: Flat| product(u[0], v[0]).wrapping_add(product(u[1], v[1]));
        let (aa, ab, bb) = (dot(a, a), dot(a, b), dot(b, b));
        let twice_ab = ab << 1;
        Self {
            frame,
            area: cross(a, b),
            corners: frame.corners(),
            along_from: [aa.wrapping_add(ab) << 1, bb.wrapping_sub(ab) << 1],
            lengths: [aa << 2, bb << 2],
            corner_squares: [
                aa.wrapping_add(twice_ab).wrapping_add(bb),
                aa.wrapping_sub(twice_ab).wrapping_add(bb),
            ],
            half_squares: [aa, bb],
        }
    }

    /// The terms the points of the row at `z` share: with `a` and `b` the half edges, a point
    /// `(x, z)` has slab values `x b_z − z b_x` and `z a_x − x a_z`, products with the half edges
    /// `x a_x + z a_z` and `x b_x + z b_z`, and square `x² + z²`.
    pub(super) fn row(&self, z: i64) -> Row {
        let [a, b] = self.frame.halves;
        Row {
            z,
            slabs: [product(z, b[0]).wrapping_neg(), product(z, a[0])],
            along: [product(z, a[1]), product(z, b[1])],
            square: product(z, z),
        }
    }

    /// The point at `x` of `row`, when it lies outside the box; `None` inside it or on its edge,
    /// where both slab values are within `quarter_area` of 0.
    pub(super) fn probe(&self, row: &Row, x: i64) -> Option<Probe> {
        let slabs = self.slabs(row, x);
        self.outside(slabs).then(|| self.probe_of(row, x, slabs))
    }

    /// The slab values of the point at `x` of `row`.
    pub(super) fn slabs(&self, row: &Row, x: i64) -> [i128; 2] {
        let [a, b] = self.frame.halves;
        [
            product(x, b[1]).wrapping_add(row.slabs[0]),
            row.slabs[1].wrapping_sub(product(x, a[1])),
        ]
    }

    /// Whether a point of these slab values lies outside the box: `|value| ≤ area` exactly when
    /// `value + area`, unsigned, is at most twice the area, one addition and one compare, where a
    /// value below `−area` wraps past it.
    pub(super) fn outside(&self, slabs: [i128; 2]) -> bool {
        slabs.iter().any(|value| {
            value.wrapping_add(self.area).cast_unsigned() > self.area.cast_unsigned() << 1
        })
    }

    /// Whether a point of these slab values lies inside the open box, off its edges: both values
    /// within the area, as `|value| < area`.
    pub(super) fn inside(&self, slabs: [i128; 2]) -> bool {
        let area = self.area.cast_unsigned();
        slabs.iter().all(|value| value.unsigned_abs() < area)
    }

    /// The point at `x` of `row`, of these slab values, outside the box.
    pub(super) fn probe_of(&self, row: &Row, x: i64, slabs: [i128; 2]) -> Probe {
        let [a, b] = self.frame.halves;
        Probe {
            x,
            z: row.z,
            slabs,
            along_a: product(x, a[0]).wrapping_add(row.along[0]),
            along_b: product(x, b[0]).wrapping_add(row.along[1]),
            square: product(x, x).wrapping_add(row.square),
        }
    }

    /// Whether the point at `x` of `row` lies closer than the reach to the box, each distance to
    /// the nearest unit: on its edge, at 0; inside the open box, below 0, as a signed distance
    /// measures it, so closer than a reach of 0 too, as a circle's center is; or nearer than the
    /// reach to an edge or a corner, the first found ending the search. Each edge is one compare against the reach, not an order
    /// compared after: that made `grid/box_spans_closer` 26 % slower on a Ryzen.
    fn closer(&self, row: &Row, x: i64, lines: &mut impl Lines) -> bool {
        let slabs = self.slabs(row, x);
        if self.outside(slabs) {
            self.closer_probe(&self.probe_of(row, x, slabs), lines)
        } else {
            lines.reach().units > 0 || self.inside(slabs)
        }
    }

    /// `closer` of a point outside the box.
    pub(super) fn closer_probe(&self, probe: &Probe, lines: &mut impl Lines) -> bool {
        let reach = lines.reach();
        (0..4).any(|edge| match self.part(probe, edge) {
            Part::Corner(corner) => self.corner_square(probe, corner) <= reach.square_limit,
            Part::Line => self.line_order(probe, edge, lines) == Ordering::Less,
        })
    }

    /// `closer` of the point `off`.
    pub(super) fn closer_point(&self, off: Flat, lines: &mut impl Lines) -> bool {
        self.closer(&self.row(off[1]), off[0], lines)
    }

    /// How the distance from `off` to the box lies against the reach: on its edge, 0; inside
    /// the open box, closer than every reach, as `closer` measures it; else the nearest of its
    /// edges' distances, each to the nearest unit and measured against the reach alone, a
    /// corner's with no root, the first closer one ending the search.
    pub(super) fn against(&self, off: Flat, lines: &mut impl Lines) -> Ordering {
        let row = self.row(off[1]);
        let slabs = self.slabs(&row, off[0]);
        if self.outside(slabs) {
            self.probe_against(&self.probe_of(&row, off[0], slabs), lines)
        } else if self.inside(slabs) {
            Ordering::Less
        } else {
            0.cmp(&lines.reach().units)
        }
    }

    /// How the distance from `probe`'s point, outside the box, lies against the reach, as
    /// `against` measures it. Inlined, so `against` keeps the probe in registers: out of line,
    /// it cost `atomic/body_box/reach` 4.5 % on a Ryzen.
    #[inline(always)]
    pub(super) fn probe_against(&self, probe: &Probe, lines: &mut impl Lines) -> Ordering {
        let units = lines.reach().units.cast_unsigned();
        let mut nearest = Ordering::Greater;
        for edge in 0..4 {
            let order = match self.part(probe, edge) {
                Part::Corner(corner) => self
                    .corner_square(probe, corner)
                    .cast_unsigned()
                    .nearest_root_cmp(units),
                Part::Line => self.line_order(probe, edge, lines),
            };
            if order == Ordering::Less {
                return order;
            }
            nearest = nearest.min(order);
        }
        nearest
    }

    /// The box's point nearest the point of `probe`, `None` for a point inside the box or on its
    /// edge: its distance, to the nearest unit, and what of the box it is; the first edge on a
    /// tie. `direction` gives a half edge's direction, by its index.
    pub(super) fn nearest(
        &self,
        probe: Option<&Probe>,
        mut direction: impl FnMut(usize) -> Direction<2>,
    ) -> Nearest {
        let Some(probe) = probe else {
            return Nearest {
                distance: 0,
                feature: Feature::Inside,
            };
        };
        // A half edge's direction only for an edge whose inside is nearest: none for a point
        // nearest a corner, one for a point beside an edge.
        let mut best: Option<Nearest> = None;
        for edge in 0..4 {
            let found = match self.part(probe, edge) {
                Part::Corner(corner) => Nearest {
                    distance: self
                        .corner_square(probe, corner)
                        .cast_unsigned()
                        .nearest_root()
                        .cast_signed(),
                    feature: Feature::Corner(corner),
                },
                Part::Line => Nearest {
                    distance: self.apart(probe, edge, direction(edge % 2)),
                    feature: Feature::Edge(edge),
                },
            };
            if best.is_none_or(|best| found.distance < best.distance) {
                best = Some(found);
            }
        }
        best.expect("four edges")
    }

    /// How the distance of `probe`'s point from the line of `edge`, to the nearest unit, lies
    /// against the reach: by the line's bounds, or else by its direction. The point's cross
    /// product with the half edge, from its start corner, is its slab value and a quarter of
    /// the area: with corners `−a − b`, `a − b`, `a + b` and `b − a`, `(off − start) × a` is
    /// `−s₁ − A` and `−s₁ + A` for edges 0 and 2, and `(off − start) × b` is `s₀ − A` and
    /// `s₀ + A` for edges 1 and 3.
    fn line_order(&self, probe: &Probe, edge: usize, lines: &mut impl Lines) -> Ordering {
        let half = edge % 2;
        let [first, second] = probe.slabs;
        let slab = if half == 0 {
            second.wrapping_neg()
        } else {
            first
        };
        let area = if edge < 2 {
            self.area.wrapping_neg()
        } else {
            self.area
        };
        let across = slab.wrapping_add(area).unsigned_abs();
        let bounds = lines.bounds(half);
        if across >= bounds.far {
            Ordering::Greater
        } else if across < bounds.near {
            Ordering::Less
        } else {
            let apart = self.apart(probe, edge, lines.direction(half));
            apart
                .cast_unsigned()
                .cmp(&lines.reach().units.cast_unsigned())
        }
    }

    /// What of `edge` is nearest `probe`'s point: its start when the point lies at or behind it,
    /// its end at or past it, else the inside.
    const fn part(&self, probe: &Probe, edge: usize) -> Part {
        let half = edge % 2;
        let along = probe.along(edge).wrapping_add(self.along_from[half]);
        if along <= 0 {
            Part::Corner(edge)
        } else if along >= self.lengths[half] {
            Part::Corner((edge + 1) % 4)
        } else {
            Part::Line
        }
    }

    /// The squared distance of `probe`'s point from `corner`.
    const fn corner_square(&self, probe: &Probe, corner: usize) -> i128 {
        probe
            .corner_term(corner)
            .wrapping_add(self.corner_squares[corner % 2])
    }

    /// The distance of `probe`'s point from the line of `edge`, along `direction`.
    fn apart(&self, probe: &Probe, edge: usize, direction: Direction<2>) -> i128 {
        let corner = self.corners[edge];
        direction.apart([
            i128::from(probe.x) - i128::from(corner[0]),
            i128::from(probe.z) - i128::from(corner[1]),
        ])
    }
}

impl Probe {
    /// `off · edge`, of each edge `2a`, `2b`, `−2a` and `−2b`.
    const fn along(&self, edge: usize) -> i128 {
        let along = match edge {
            0 => self.along_a,
            1 => self.along_b,
            2 => self.along_a.wrapping_neg(),
            _ => self.along_b.wrapping_neg(),
        };
        along.wrapping_add(along)
    }

    /// `off · off − 2 off · corner`, of each corner `−a − b`, `a − b`, `a + b` and `b − a`.
    const fn corner_term(&self, corner: usize) -> i128 {
        let to_corner = match corner {
            0 => self.along_a.wrapping_add(self.along_b).wrapping_neg(),
            1 => self.along_a.wrapping_sub(self.along_b),
            2 => self.along_a.wrapping_add(self.along_b),
            _ => self.along_b.wrapping_sub(self.along_a),
        };
        self.square.wrapping_sub(to_corner.wrapping_add(to_corner))
    }
}

impl LineBounds {
    /// The bounds of a half edge whose squared length is `square`, against `reach`: its length
    /// lies from the floor root to the ceiling one, so `far` takes the ceiling and `near` the
    /// floor. A reach and a half edge within the world's bound keep each below 2¹⁰², so the
    /// wrapping steps skip the checks a release build makes.
    pub(super) fn of(square: i128, reach: i128) -> Self {
        debug_assert!(square > 0 && reach >= 0);
        let square = square.cast_unsigned();
        let root = square.floor_root();
        let ceiling = root.wrapping_add(u128::from(root.wrapping_mul(root) < square));
        let reach = reach.cast_unsigned();
        Self {
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
    pub(super) const fn of(units: i128) -> Self {
        debug_assert!(units >= 0);
        Self {
            units,
            square_limit: if units > 0 { units * units - units } else { -1 },
        }
    }
}
