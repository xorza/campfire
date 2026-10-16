use std::cmp::Ordering;

use campfire_math::{Direction, Flat, Num, RoundedRoot, Rounding, SinCos, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::approach::Approach;
use crate::body_bound::MAX_BOUND;
use crate::body_box::closer::{Closer, HalfDirections, LinesOnDemand};
use crate::body_box::closure::Closure;
use crate::body_box::edges::{Edges, LineBounds, Reach};
use crate::polygon::Polygon;

#[cfg(feature = "internals")]
pub(crate) mod bench;
mod closer;
mod closure;
mod edges;

/// A box body: a parallelogram on the ground plane around its unit's position, held as its two half
/// edges `a` and `b`, each `[x, z]`, rounded once as it was made. Its corners are the position plus
/// `−a − b`, `a − b`, `a + b` and `−a + b`, counterclockwise, and its edges run from each corner to
/// the next. Every test reads them in integers, whether two shapes meet exactly and each distance
/// to the nearest unit (09-determinism-core.md, Exact where it fits), so the box is the same shape
/// to every test on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BodyBox {
    half: [[Num; 2]; 2],
    /// The radius of the least circle round the position that holds it, rounded up: derived from
    /// the half edges.
    #[serde(skip)]
    bound: Num,
}

/// A box in raw integers: its half edges in bits, or in halves of a bit at twice the scale.
/// Every test lives here, so it serves both scales alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frame {
    halves: [Flat; 2],
}

/// What of a box is nearest a point: the point lies inside it or on its edge, or nearest the
/// inside of an edge, or a corner, each by its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feature {
    Inside,
    Edge(usize),
    Corner(usize),
}

/// A box's point nearest a point: its distance, to the nearest unit, and what of the box it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Nearest {
    distance: i128,
    feature: Feature,
}

impl BodyBox {
    /// The least size: 2⁻¹⁰ m, about a millimeter, many bits above the rounding of a half
    /// edge, so the rounded box never lies flat at any angle.
    pub(crate) const MIN_SIZE: Num = Num::from_bits(1 << (Num::FRAC_BITS - 10));
    /// The longest diagonal, unrounded: 4,094 m, so the corners stay within `MAX_BOUND` of
    /// the position whatever the rounding.
    pub(crate) const MAX_DIAGONAL: Num = Num::int(4094);

    /// The box of `size`, `[width, height]` in meters along `a` and `b`, turned by `angle`
    /// degrees as a unit's facing turns, from `x` towards `−z`: `a` along `(cos, −sin)` and `b`
    /// along `(sin, cos)`, so its model, which the client turns by the same angle, covers it.
    /// Each component of a half edge is the exact product of half a size and a cosine or a sine,
    /// rounded once, to nearest, ties to even. A multiple of 90° turns by exact sines; any other
    /// angle by those `sin_cos` gives. `None` for a size below `MIN_SIZE`, or for a diagonal past
    /// `MAX_DIAGONAL`.
    pub fn new(size: [Num; 2], angle: Num) -> Option<Self> {
        let [width, height] = size;
        if width < Self::MIN_SIZE || height < Self::MIN_SIZE {
            return None;
        }
        let square = |num: Num| {
            let bits = u128::from(num.to_bits().unsigned_abs());
            bits * bits
        };
        // Each square is below 2¹²⁶, so the sum fits.
        if square(width) + square(height) > square(Self::MAX_DIAGONAL) {
            return None;
        }
        let SinCos { sin, cos } = SinCos::of_degrees(angle);
        let half = |size: Num, by: Num| {
            size.checked_mul_div(by, Num::int(2), Rounding::NearestEven)
                .expect("half a size within 4,094 m times at most 1 fits")
        };
        let made = Self::of_halves([
            [half(width, cos), -half(width, sin)],
            [half(height, sin), half(height, cos)],
        ]);
        debug_assert!(made.is_some(), "a box of the least size is not flat");
        made
    }

    /// The box of the half edges `half`; `None` for one that lies flat or turns clockwise, or
    /// whose corners reach past `MAX_BOUND`.
    fn of_halves(half: [[Num; 2]; 2]) -> Option<Self> {
        // A component past the reach makes a corner past it; refused first, the products below
        // stay within i128 whatever an untrusted snapshot holds.
        let within = |num: &Num| num.to_bits().unsigned_abs() <= MAX_BOUND.to_bits().unsigned_abs();
        if !half.iter().flatten().all(within) {
            return None;
        }
        let frame = Frame {
            halves: half.map(Flat::from_nums),
        };
        if frame.quarter_area() <= 0 {
            return None;
        }
        let [a, b] = frame.halves;
        let farthest = (a + b).length_squared().max((a - b).length_squared());
        let bound = Num::from_bits(i64::try_from(farthest.ceil_root()).ok()?);
        (bound <= MAX_BOUND).then_some(Self { half, bound })
    }

    pub const fn half_edges(&self) -> [[Num; 2]; 2] {
        self.half
    }

    /// The radius of the least circle round its position that holds it, rounded up.
    pub const fn bound(&self) -> Num {
        self.bound
    }

    /// How far it reaches from its position along x and along z.
    pub fn extent(&self) -> [Num; 2] {
        // Each half edge's component is within `MAX_BOUND`, so the sum of two magnitudes cannot
        // wrap, and the wrapping steps skip the checks a release build makes.
        let [a, b] = self.half;
        [0, 1].map(|axis| {
            let magnitude = a[axis]
                .to_bits()
                .unsigned_abs()
                .wrapping_add(b[axis].to_bits().unsigned_abs());
            Num::from_bits(magnitude.cast_signed())
        })
    }

    /// Whether `pos` lies closer than `reach` to the box at `center`, exactly: a point at `reach`
    /// is not closer.
    pub fn closer(&self, center: Vec3, pos: Vec3, reach: Num) -> bool {
        self.nearest(center, pos, reach) == Ordering::Less
    }

    /// How the distance from `pos` to the box at `center` lies against `reach`: past it when `pos`
    /// lies past the box's extent and the reach along an axis, as the distance is at least that
    /// gap; else as the nearest of the box's features lies against it.
    pub(crate) fn nearest(&self, center: Vec3, pos: Vec3, reach: Num) -> Ordering {
        let off = flat(pos) - flat(center);
        let extent = self.extent();
        let reach_bits = reach.to_bits();
        let far = |axis: usize| {
            extent[axis]
                .to_bits()
                .saturating_add(reach_bits)
                .cast_unsigned()
        };
        if (0..2).any(|axis| off[axis].unsigned_abs() > far(axis)) {
            return Ordering::Greater;
        }
        let edges = Edges::of(self.frame());
        let mut lines = LinesOnDemand::new(&edges, Reach::of(i128::from(reach.to_bits())));
        edges.against(off, &mut lines)
    }

    /// The point of the box at `center` nearest `pos`, each coordinate rounded to the nearest bit:
    /// `pos` itself inside.
    pub fn nearest_point(&self, center: Vec3, pos: Vec3) -> Vec3 {
        let frame = self.frame();
        let off = flat(pos) - flat(center);
        let corners = frame.corners();
        let point = match frame
            .nearest_to(off, &mut HalfDirections::new(frame))
            .feature
        {
            Feature::Inside => off,
            Feature::Corner(corner) => corners[corner],
            Feature::Edge(edge) => {
                let start = corners[edge];
                let along_edge = corners[(edge + 1) % 4] - start;
                let along = (off - start).dot(along_edge);
                let length = along_edge.dot(along_edge);
                start + along_edge.map(|axis| Rounding::NearestEven.divide(axis * along, length))
            }
        };
        let ground = point + flat(center);
        Vec3::new(Num::from_bits(ground[0]), pos.y, Num::from_bits(ground[1]))
    }

    /// How the straight path from `from` to `to` comes to the box at `center`, against `reach`: a
    /// path that meets the box comes nearest where it enters it, or at its start inside it, at 0,
    /// and closer than a reach of 0 too when it enters the open box, as `Edges::against` measures a
    /// point inside it; one that does not, at its nearest point, the first of them along the path
    /// where a stretch is nearest. Each distance and place is to the nearest bit.
    pub(crate) fn approach(&self, center: Vec3, from: Vec3, to: Vec3, reach: Num) -> Approach {
        let frame = self.frame();
        let start = flat(from) - flat(center);
        let path = flat(to) - flat(from);
        let reach = i128::from(reach.to_bits());
        let square = path.dot(path);
        let length = square.cast_unsigned().nearest_root().cast_signed();
        if let Some(place) = frame.entry(start, path, length) {
            let inside = reach == 0 && frame.meets_path(start, path, Closure::Open);
            return Approach {
                nearest: if inside {
                    Ordering::Less
                } else {
                    0.cmp(&reach)
                },
                place: bits(place),
            };
        }
        let mut directions = HalfDirections::new(frame);
        let mut best = (frame.nearest_to(start, &mut directions).distance, 0);
        let mut take = |candidate: (i128, i128)| best = best.min(candidate);
        take((
            frame.nearest_to(start + path, &mut directions).distance,
            length,
        ));
        // Apart from the box, a path comes nearest at one of its ends, or where a corner's
        // nearest point lies inside it, measured by the path's direction, made only then.
        let mut direction = None;
        for corner in frame.corners() {
            let off = corner - start;
            let along = off.dot(path);
            if 0 < along && along < square {
                let direction = direction.get_or_insert_with(|| {
                    Direction::of(path.to_array()).expect("a path past a corner has a length")
                });
                let off = off.to_array();
                take((direction.apart(off), direction.along(off)));
            }
        }
        Approach {
            nearest: best.0.cmp(&reach),
            place: bits(best.1),
        }
    }

    /// Whether the straight path from `from` to `to` comes closer than `reach` to the box at
    /// `center`, as `approach` finds it, with no place: a path that meets the closed box is at 0,
    /// closer than a reach of 0 only when it enters the open box, and one that does not comes
    /// nearest at an end, or where a corner's nearest point lies inside it, so it is closer when
    /// one of these is.
    pub(crate) fn comes_closer(&self, center: Vec3, from: Vec3, to: Vec3, reach: Num) -> bool {
        if reach < Num::ZERO {
            return false;
        }
        let frame = self.frame();
        let start = flat(from) - flat(center);
        let path = flat(to) - flat(from);
        if reach == Num::ZERO {
            return frame.meets_path(start, path, Closure::Open);
        }
        let reach = i128::from(reach.to_bits());
        let edges = Edges::of(frame);
        let mut lines = LinesOnDemand::new(&edges, Reach::of(reach));
        if frame.meets_path(start, path, Closure::Closed)
            || edges.closer_point(start, &mut lines)
            || edges.closer_point(start + path, &mut lines)
        {
            return true;
        }
        // A corner beside the path is measured from the path's line: its bounds decide most,
        // and the path's direction, made only then, the rest. A path of no length has none.
        let length = path.dot(path);
        if length == 0 {
            return false;
        }
        let bounds = LineBounds::of(length, reach);
        let mut direction = None;
        edges.corners.into_iter().any(|corner| {
            let off = corner - start;
            let along = off.dot(path);
            if along <= 0 || along >= length {
                return false;
            }
            let across = cross(off, path).unsigned_abs();
            if across >= bounds.far {
                false
            } else if across < bounds.near {
                true
            } else {
                let direction = *direction.get_or_insert_with(|| {
                    Direction::of(path.to_array()).expect("a path of a length")
                });
                direction.apart(off.to_array()) < reach
            }
        })
    }

    /// `point` moved out of the box at `center` when a body of `radius` there overlaps it, as a box
    /// leaves all of an overlap to the body: from the box's nearest point straight away until
    /// the body's edge touches the box, or, from inside, out through the nearest edge, the first
    /// edge on a tie; the move goes `PUSH_SLACK` past touching, each coordinate rounded away from
    /// the box, so the body ends apart, and the next measure sees it so. `None` when it does not
    /// overlap, each distance to the nearest bit: touching is not overlap.
    pub fn push_out(&self, center: Vec3, point: Vec3, radius: Num) -> Option<Vec3> {
        let frame = self.frame();
        let off = Flat::ground(point) - flat(center);
        // A point outside the box at least `radius` away is not overlapped: most points a search
        // finds lie so, and the lines' bounds decide it, as `nearest_to`'s distance would, with
        // no direction.
        let edges = Edges::of(frame);
        let mut lines = LinesOnDemand::new(&edges, Reach::of(i128::from(radius.to_bits())));
        let probe = edges.probe(&edges.row(off[1]), off[0]);
        if probe.is_some_and(|probe| edges.probe_against(&probe, &mut lines) != Ordering::Less) {
            return None;
        }
        let directions = &mut lines.directions;
        let Nearest { distance, feature } =
            edges.nearest(probe.as_ref(), |half| directions.of_half(half));
        let moved = match feature {
            Feature::Inside => {
                let edge = frame.nearest_edge_inside(off, directions);
                frame.out_through(edge, off, radius, directions)
            }
            _ if distance >= i128::from(radius.to_bits()) => return None,
            Feature::Edge(edge) => frame.out_through(edge, off, radius, directions),
            Feature::Corner(corner) => {
                // Off a corner the push runs along the way from it, across the line through
                // the corner square to that way.
                let corner = frame.corners()[corner];
                let away = Direction::of((off - corner).to_array())
                    .expect("a point off a corner lies apart from it");
                touching(off, away, corner, radius)
            }
        };
        let ground = moved + flat(center);
        Some(Vec3::new(
            Num::from_bits(ground[0]),
            point.y,
            Num::from_bits(ground[1]),
        ))
    }

    /// Whether the insides of the box at `center` and `other` at `other_center` share a point:
    /// touching is not overlap.
    pub fn overlaps(&self, center: Vec3, other: &Self, other_center: Vec3) -> bool {
        let apart = flat(other_center) - flat(center);
        self.frame().meets(apart, other.frame(), Closure::Open)
    }

    /// How the distance between the box at `center` and `other` at `other_center` lies against
    /// `reach`: 0 when they touch, and closer than every reach when they overlap, as
    /// `Edges::against` measures a point inside a box.
    pub(crate) fn nearest_box(
        &self,
        center: Vec3,
        other: &Self,
        other_center: Vec3,
        reach: Num,
    ) -> Ordering {
        let apart = flat(other_center) - flat(center);
        // A gap along an axis past both extents and the reach is at least a unit past the
        // reach, so is every distance between the boxes, and so their order: most boxes a
        // search finds lie so.
        let (our_extent, their_extent) = (self.extent(), other.extent());
        let far = |axis: usize| {
            let extents = our_extent[axis].to_bits() + their_extent[axis].to_bits();
            extents.saturating_add(reach.to_bits()).cast_unsigned()
        };
        if (0..2).any(|axis| apart[axis].unsigned_abs() > far(axis)) {
            return Ordering::Greater;
        }
        let (ours, theirs) = (self.frame(), other.frame());
        let reach = i128::from(reach.to_bits());
        if ours.meets(apart, theirs, Closure::Closed) {
            let overlap = reach == 0 && ours.meets(apart, theirs, Closure::Open);
            return if overlap {
                Ordering::Less
            } else {
                0.cmp(&reach)
            };
        }
        // Apart, two convex shapes come nearest at a corner of one: the nearest corner's order
        // against the reach is the least of the eight corners' orders, each as `Edges::against`
        // measures it, and the first closer one ends the search.
        let (our_edges, their_edges) = (Edges::of(ours), Edges::of(theirs));
        let reach = Reach::of(reach);
        let mut our_lines = LinesOnDemand::new(&our_edges, reach);
        let mut their_lines = LinesOnDemand::new(&their_edges, reach);
        let mut nearest = Ordering::Greater;
        for corner in our_edges.corners {
            nearest = nearest.min(their_edges.against(corner - apart, &mut their_lines));
            if nearest == Ordering::Less {
                return nearest;
            }
        }
        for corner in their_edges.corners {
            nearest = nearest.min(our_edges.against(corner + apart, &mut our_lines));
            if nearest == Ordering::Less {
                return nearest;
            }
        }
        nearest
    }

    /// Whether the insides of the box at `center` and `polygon` share a point: an edge of the
    /// polygon passes through the open box, or the box lies inside the polygon, which its
    /// center then tells. Touching is not overlap.
    pub fn overlaps_polygon(&self, center: Vec3, polygon: &Polygon) -> bool {
        let frame = self.frame();
        let points = polygon.points();
        let crosses = (0..points.len()).any(|at| {
            let next = points[(at + 1) % points.len()];
            let start = Flat::from_nums(points[at]) - flat(center);
            let path = Flat::from_nums(next) - Flat::from_nums(points[at]);
            frame.meets_path(start, path, Closure::Open)
        });
        crosses || polygon.holds_point([center.x, center.z])
    }

    /// The test of whether a point, from the box's position in halves of a bit, comes closer
    /// than `reach` to the box, exactly: as a grid's cell centers, whole only in halves, see it.
    pub(crate) fn closer_twice(&self, reach: Num) -> Closer {
        let reach = reach
            .checked_mul_int(2)
            .expect("a reach within a body and a walker doubles");
        Closer::new(self.frame_twice(), i128::from(reach.to_bits()))
    }

    /// The test of whether the insides of the box and of a square share a point, from the
    /// square's center: its offset from the box's position, and the half side `half`, both in
    /// halves of a bit, as a grid's cells lie. Touching is not overlap.
    pub(crate) fn overlaps_square_twice(&self, half: i128) -> impl Fn(Flat) -> bool {
        let square = Frame {
            halves: [Flat::new(half, 0), Flat::new(0, half)],
        };
        let frame = self.frame_twice();
        move |off| frame.meets(off, square, Closure::Open)
    }

    fn frame(&self) -> Frame {
        Frame {
            halves: self.half.map(Flat::from_nums),
        }
    }

    /// The box at twice the scale, in halves of a bit.
    fn frame_twice(&self) -> Frame {
        Frame {
            halves: self.half.map(|edge| Flat::from_nums(edge) * 2),
        }
    }
}

/// A snapshot is untrusted, so half edges that lie flat, turn clockwise, or reach past 2,048 m fail
/// to decode; the bound is derived again.
impl<'de> Deserialize<'de> for BodyBox {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            half: [[Num; 2]; 2],
        }
        let Fields { half } = Fields::deserialize(deserializer)?;
        Self::of_halves(half).ok_or_else(|| {
            D::Error::custom("a box's half edges turn counterclockwise, within 2,048 m")
        })
    }
}

impl Frame {
    /// The cross product of the half edges, positive: a quarter of the box's area.
    const fn quarter_area(&self) -> i128 {
        let [a, b] = self.halves;
        a.cross(b)
    }

    /// The corners from the position, counterclockwise.
    fn corners(&self) -> [Flat; 4] {
        // Each half edge is within `MAX_BOUND` at twice the scale, so no sum or difference wraps,
        // and the wrapping steps skip the checks a release build makes.
        let [a, b] = self.halves;
        let flat = |x: i64, z: i64| Flat::from_nums([Num::from_bits(x), Num::from_bits(z)]);
        let sum = [a[0].wrapping_add(b[0]), a[1].wrapping_add(b[1])];
        let difference = [a[0].wrapping_sub(b[0]), a[1].wrapping_sub(b[1])];
        [
            flat(sum[0].wrapping_neg(), sum[1].wrapping_neg()),
            flat(difference[0], difference[1]),
            flat(sum[0], sum[1]),
            flat(difference[0].wrapping_neg(), difference[1].wrapping_neg()),
        ]
    }

    /// The two slab values of `off`, each `cross` of it with a half edge: it lies within the
    /// box when both are within `quarter_area` of 0.
    fn slabs(&self, off: Flat) -> [i128; 2] {
        let [a, b] = self.halves;
        [cross(off, b), cross(a, off)]
    }

    /// The directions of the half edges: edges 0 and 2 run along the first, 1 and 3 along the
    /// second.
    fn directions(&self) -> [Direction<2>; 2] {
        [0, 1].map(|half| self.direction(half))
    }

    /// The direction of half edge `half`, 0 or 1.
    fn direction(&self, half: usize) -> Direction<2> {
        Direction::of(self.halves[half].to_array()).expect("a box's half edge has a length")
    }

    /// The nearest point's distance from `off`, to the nearest unit, and what of the box it is;
    /// the first edge on a tie.
    fn nearest_to(&self, off: Flat, directions: &mut HalfDirections) -> Nearest {
        let edges = Edges::of(*self);
        let probe = edges.probe(&edges.row(off[1]), off[0]);
        edges.nearest(probe.as_ref(), |half| directions.of_half(half))
    }

    /// Whether the path from `start`, along `path`, both ends its own, meets the box of
    /// `closure`, by separating axes, with no fraction: a box and a segment are convex, so they
    /// meet unless one of the box's two slab axes or the path's normal separates them. Along a
    /// slab the path runs from its start's value to its end's; across the path, the box spans
    /// its half edges' crosses with the path either side of 0, and the path lies at its start's.
    /// A path of no length has no normal, and only the slabs separate it. Within the world's
    /// bound each value stays below 2¹⁰⁰, so the wrapping steps skip the checks a release build
    /// makes. Closed, it meets as `entry` finds.
    fn meets_path(&self, start: Flat, path: Flat, closure: Closure) -> bool {
        let area = closure
            .bound(self.quarter_area().cast_unsigned())
            .cast_signed();
        let (from, along) = (self.slabs(start), self.slabs(path));
        let slab_apart = (0..2).any(|slab| {
            let (from, to) = (from[slab], from[slab].wrapping_add(along[slab]));
            from.min(to) > area || from.max(to) < area.wrapping_neg()
        });
        let [a, b] = self.halves;
        let across = |half: Flat| cross(path, half).unsigned_abs();
        let still = path[0] == 0 && path[1] == 0;
        let spread = closure.bound(across(a).wrapping_add(across(b)));
        !slab_apart && (still || across(start) <= spread)
    }

    /// The edge nearest `off`, which lies inside the box, each edge's distance to the nearest
    /// unit; the first on a tie.
    fn nearest_edge_inside(&self, off: Flat, directions: &mut HalfDirections) -> usize {
        let corners = self.corners();
        let mut depth = |edge: usize| {
            directions
                .of_half(edge % 2)
                .apart((off - corners[edge]).to_array())
        };
        (0..4).min_by_key(|&edge| depth(edge)).expect("four edges")
    }

    /// Where `off` goes along `edge`'s outward normal to leave a body of `radius` touching the
    /// edge's line from outside.
    fn out_through(
        &self,
        edge: usize,
        off: Flat,
        radius: Num,
        directions: &mut HalfDirections,
    ) -> Flat {
        touching(off, directions.normal(edge), self.corners()[edge], radius)
    }

    /// Whether the boxes of `closure` meet: no axis of either separates them. `other`'s
    /// position lies `apart` from this one's.
    fn meets(&self, apart: Flat, other: Self, closure: Closure) -> bool {
        !self.separates(apart, other, closure) && !other.separates(-apart, *self, closure)
    }

    /// Whether one of this box's slab axes separates it from `other`, whose position lies
    /// `apart` from this one's.
    fn separates(&self, apart: Flat, other: Self, closure: Closure) -> bool {
        let area = self.quarter_area();
        let [their_a, their_b] = other.halves;
        let center = self.slabs(apart);
        let (a_spread, b_spread) = (self.slabs(their_a), self.slabs(their_b));
        // `|center| > bound` exactly when `center + bound`, unsigned, passes twice the bound,
        // where a center below `−bound` wraps past it. A box and an offset within the world's
        // bound keep each value below 2¹⁰⁰, so no sum wraps.
        (0..2).any(|slab| {
            let limit = area
                .cast_unsigned()
                .wrapping_add(a_spread[slab].unsigned_abs())
                .wrapping_add(b_spread[slab].unsigned_abs());
            let bound = closure.bound(limit);
            center[slab].cast_unsigned().wrapping_add(bound) > bound << 1
        })
    }

    /// The place where the path from `start`, along `path`, `length` long to the nearest unit,
    /// first meets the closed box, or none: whether it meets is `meets_path`'s, exact, and the
    /// place the latest of the slabs' entries, from 0 to the length, each rounded once. A slab's
    /// value below 2⁸¹ and a length below 2⁴⁶ keep each product below 2¹²⁷.
    fn entry(&self, start: Flat, path: Flat, length: i128) -> Option<i128> {
        if !self.meets_path(start, path, Closure::Closed) {
            return None;
        }
        let area = self.quarter_area();
        let (at_start, along) = (self.slabs(start), self.slabs(path));
        let enter = (0..2)
            .filter(|&slab| along[slab] != 0)
            .map(|slab| {
                let edge = if along[slab] > 0 { -area } else { area };
                Rounding::NearestEven.divide((edge - at_start[slab]) * length, along[slab])
            })
            .fold(0, i128::max);
        Some(enter.min(length))
    }
}

/// `a · b`, exact: one widening product of two `i64`s, which cannot overflow.
const fn product(a: i64, b: i64) -> i128 {
    (a as i128).wrapping_mul(b as i128)
}

/// `Flat::cross`, for a box and an offset within the world's bound, at twice the scale, whose
/// products stay below 2¹⁰⁰: their difference cannot wrap, and the wrapping step skips the
/// check a release build makes.
fn cross(a: Flat, b: Flat) -> i128 {
    product(a[0], b[1]).wrapping_sub(product(a[1], b[0]))
}

/// Where `pos` stands on the ground plane, in a `Num`'s bits.
const fn flat(pos: Vec3) -> Flat {
    Flat::ground(pos)
}

/// The number of `bits`, a place on a path within the world's bound.
fn bits(bits: i128) -> Num {
    Num::from_bits(i64::try_from(bits).expect("a place within the world's bound"))
}

/// How far a push of a body out of a box goes past touching, in bits: the most a measure of a
/// distance by a `Direction`, or by a nearest root, lies from the true one, 0.51 of a bit, rounded
/// up, so the next measure sees the body apart (09-determinism-core.md, A push rounds away from
/// what it leaves).
const PUSH_SLACK: i128 = 1;

/// `off` moved along `normal` until a body of `radius` there touches, from outside, the line
/// across `normal` through `on`, and `PUSH_SLACK` past it: by the radius less how far `off`
/// lies beyond that line, each axis rounded away from the box. As the box grown by the radius
/// is convex, a point that lies on the outer side of the line through the touching point across
/// `normal` does not overlap the box, so the end lies apart, within three bits of touching.
fn touching(off: Flat, direction: Direction<2>, on: Flat, radius: Num) -> Flat {
    let beyond = direction.along((off - on).to_array());
    let reach = i128::from(radius.to_bits()) - beyond + PUSH_SLACK;
    debug_assert!(
        reach > 0,
        "a body that overlaps the box lies within its radius of the line"
    );
    off + Flat::from_array(direction.scaled_away(reach))
}

#[cfg(test)]
mod tests;
