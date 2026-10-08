use std::cmp::Ordering;

use campfire_math::{FloorRoot, Num, SinCos, U256, Vec3};
use campfire_sim::Position;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::geometry::approach::Approach;
use crate::geometry::fraction::Fraction;
use crate::geometry::polygon::Polygon;
use crate::geometry::squared_distance::SquaredDistance;

#[cfg(feature = "bench")]
pub(crate) mod bench;

/// A point or a vector on the ground plane, `[x, z]` in a `Num`'s bits, or in halves of a bit
/// at twice the scale.
type Flat = [i128; 2];

/// A box body: a parallelogram on the ground plane around its unit's position, held as its two
/// half edges `a` and `b`, each `[x, z]`, rounded once as it was made. Its corners are the
/// position plus `−a − b`, `a − b`, `a + b` and `−a + b`, counterclockwise, and its edges run
/// from each corner to the next. Every test reads them exactly, in wide integers, so the box is
/// the same shape to every test on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct BodyBox {
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

/// A box's point nearest a point: its squared distance, and what of the box it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Nearest {
    distance: SquaredDistance,
    feature: Feature,
}

/// The values of `t` for which `q₀ + t·v` lies within one of a box's slabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    All,
    Empty,
    Between { low: Fraction, high: Fraction },
}

/// The values of `t` within every slab: from `low` to `high`, `None` an end with no bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Clip {
    low: Option<Fraction>,
    high: Option<Fraction>,
}

impl BodyBox {
    /// The least size: 2⁻¹⁰ m, about a millimeter, many bits above the rounding of a half
    /// edge, so the rounded box never lies flat at any angle.
    pub(crate) const MIN_SIZE: Num = Num::from_bits(1 << (Num::FRAC_BITS - 10));
    /// The longest diagonal, unrounded: 126 m, so the corners stay within `MAX_REACH` of the
    /// position whatever the rounding.
    pub(crate) const MAX_DIAGONAL: Num = Num::int(126);
    /// The farthest a corner lies from the position: the widest body's reach.
    const MAX_REACH: Num = Num::int(64);

    /// The box of `size`, `[width, height]` in meters along `a` and `b`, turned by `angle`
    /// degrees counterclockwise: each component of a half edge is the exact product of half a
    /// size and a cosine or a sine, rounded once, to nearest, ties to even. A multiple of 90°
    /// turns by exact sines; any other angle by those `sin_cos` gives. `None` for a size below
    /// `MIN_SIZE`, or for a diagonal past `MAX_DIAGONAL`.
    pub(crate) fn new(size: [Num; 2], angle: Num) -> Option<BodyBox> {
        let [width, height] = size;
        if width < BodyBox::MIN_SIZE || height < BodyBox::MIN_SIZE {
            return None;
        }
        let square = |num: Num| {
            let bits = u128::from(num.to_bits().unsigned_abs());
            bits * bits
        };
        // Each square is below 2¹²⁶, so the sum fits.
        if square(width) + square(height) > square(BodyBox::MAX_DIAGONAL) {
            return None;
        }
        let SinCos { sin, cos } = BodyBox::turn(angle);
        let half = |size: Num, by: Num| {
            size.checked_mul_div_int(by, 2)
                .expect("half a size within 126 m times at most 1 fits")
        };
        let made = BodyBox::of_halves([
            [half(width, cos), half(width, sin)],
            [-half(height, sin), half(height, cos)],
        ]);
        debug_assert!(made.is_some(), "a box of the least size is not flat");
        made
    }

    /// The box of the half edges `half`; `None` for one that lies flat or turns clockwise, or
    /// whose corners reach past `MAX_REACH`.
    fn of_halves(half: [[Num; 2]; 2]) -> Option<BodyBox> {
        // A component past the reach makes a corner past it; refused first, the products below
        // stay within i128 whatever an untrusted snapshot holds.
        let within =
            |num: &Num| num.to_bits().unsigned_abs() <= BodyBox::MAX_REACH.to_bits().unsigned_abs();
        if !half.iter().flatten().all(within) {
            return None;
        }
        let frame = Frame {
            halves: half.map(bits),
        };
        if frame.quarter_area() <= 0 {
            return None;
        }
        let [a, b] = frame.halves;
        let farthest = dot(add(a, b), add(a, b)).max(dot(sub(a, b), sub(a, b)));
        let root = farthest.unsigned_abs().floor_root();
        let up = root + u128::from(root * root < farthest.unsigned_abs());
        let bound = Num::from_bits(i64::try_from(up).ok()?);
        (bound <= BodyBox::MAX_REACH).then_some(BodyBox { half, bound })
    }

    /// The sine and the cosine of `angle` degrees, exact at each multiple of 90°.
    fn turn(angle: Num) -> SinCos {
        const FULL: i64 = 360 << Num::FRAC_BITS;
        const QUARTER: i64 = 90 << Num::FRAC_BITS;
        let degrees = angle.to_bits().rem_euclid(FULL);
        if degrees % QUARTER == 0 {
            return match degrees / QUARTER {
                0 => SinCos {
                    sin: Num::ZERO,
                    cos: Num::ONE,
                },
                1 => SinCos {
                    sin: Num::ONE,
                    cos: Num::ZERO,
                },
                2 => SinCos {
                    sin: Num::ZERO,
                    cos: -Num::ONE,
                },
                _ => SinCos {
                    sin: -Num::ONE,
                    cos: Num::ZERO,
                },
            };
        }
        let radians = Num::from_bits(degrees)
            .checked_mul_div_int(Num::PI, 180)
            .expect("an angle below 360° fits in radians");
        radians.sin_cos()
    }

    pub(crate) const fn half_edges(&self) -> [[Num; 2]; 2] {
        self.half
    }

    /// The radius of the least circle round its position that holds it, rounded up.
    pub(crate) const fn bound(&self) -> Num {
        self.bound
    }

    /// How far it reaches from its position along x and along z.
    pub(crate) fn extent(&self) -> [Num; 2] {
        let [a, b] = self.half;
        [0, 1].map(|axis| Num::from_bits(a[axis].to_bits().abs() + b[axis].to_bits().abs()))
    }

    /// The squared distance from `at` to the nearest point of the box at `centre`; 0 inside.
    pub(crate) fn distance(&self, centre: Position, at: Position) -> SquaredDistance {
        self.frame()
            .nearest_to(sub(flat(at), flat(centre)))
            .distance
    }

    /// How the distance from `at` to the box at `centre` lies against `reach`.
    pub(crate) fn nearest(&self, centre: Position, at: Position, reach: Num) -> Ordering {
        self.distance(centre, at).cmp(&SquaredDistance::of(reach))
    }

    /// The point of the box at `centre` nearest `at`, each coordinate rounded to the nearest bit:
    /// `at` itself inside.
    pub(crate) fn nearest_point(&self, centre: Position, at: Position) -> Position {
        let frame = self.frame();
        let off = sub(flat(at), flat(centre));
        let corners = frame.corners();
        let point = match frame.nearest_to(off).feature {
            Feature::Inside => off,
            Feature::Corner(corner) => corners[corner],
            Feature::Edge(edge) => {
                let start = corners[edge];
                let along_edge = sub(corners[(edge + 1) % 4], start);
                let along = dot(sub(off, start), along_edge);
                let length = dot(along_edge, along_edge);
                add(start, along_edge.map(|axis| divide(axis * along, length)))
            }
        };
        let ground = add(point, flat(centre));
        let place = |bits: i128| Num::from_bits(i64::try_from(bits).expect("within the bound"));
        let at = Vec3::new(place(ground[0]), at.get().y, place(ground[1]));
        Position::new(at).expect("a point of a box within the bound")
    }

    /// How the straight path from `from` to `to` comes to the box at `centre`, against `reach`:
    /// a path that meets the box comes nearest where it enters it, or at its start inside it;
    /// one that does not, at its nearest point, the first of them along the path where a stretch
    /// is nearest.
    pub(crate) fn approach(
        &self,
        centre: Position,
        from: Position,
        to: Position,
        reach: Num,
    ) -> Approach {
        let frame = self.frame();
        let start = sub(flat(from), flat(centre));
        let path = sub(flat(to), flat(from));
        let reach = SquaredDistance::of(reach);
        if let Some(share) = frame.entry(start, path) {
            return Approach {
                nearest: SquaredDistance::ZERO.cmp(&reach),
                share,
            };
        }
        let mut best = (frame.nearest_to(start).distance, Fraction::ZERO);
        let mut take = |candidate: (SquaredDistance, Fraction)| {
            if candidate.0 < best.0 || candidate.0 == best.0 && candidate.1 < best.1 {
                best = candidate;
            }
        };
        take((frame.nearest_to(add(start, path)).distance, Fraction::ONE));
        // Apart from the box, a path comes nearest at one of its ends, or where a corner's
        // nearest point lies inside it.
        let length = dot(path, path);
        for corner in frame.corners() {
            let off = sub(corner, start);
            let along = dot(off, path);
            if 0 < along && along < length {
                let across = cross(path, off).unsigned_abs();
                let square = U256::product(across, across);
                let distance = SquaredDistance::new(square, length.unsigned_abs());
                take((distance, Fraction::new(along, length)));
            }
        }
        Approach {
            nearest: best.0.cmp(&reach),
            share: best.1,
        }
    }

    /// `at` moved out of the box at `centre` when a body of `radius` there overlaps it, as a box
    /// leaves all of an overlap to the body: from the box's nearest point straight away until
    /// the body's edge touches the box, or, from inside, out through the nearest edge, the first
    /// edge on a tie. `None` when it does not overlap: touching is not overlap.
    pub(crate) fn push_out(&self, centre: Position, at: Vec3, radius: Num) -> Option<Vec3> {
        let frame = self.frame();
        let off = sub(flat_vec(at), flat(centre));
        let Nearest { distance, feature } = frame.nearest_to(off);
        let radius_bits = i128::from(radius.to_bits());
        let moved = match feature {
            Feature::Inside => frame.out_through(frame.nearest_edge_inside(off), off, radius),
            _ if distance >= SquaredDistance::of(radius) => return None,
            Feature::Edge(edge) => frame.out_through(edge, off, radius),
            Feature::Corner(corner) => {
                // `radius` along the way from the corner: within it, the way is shorter than
                // 64 m, so its fine root fits.
                let start = frame.corners()[corner];
                let away = sub(off, start);
                let length = fine_root(dot(away, away).unsigned_abs());
                add(
                    start,
                    away.map(|along| divide((radius_bits * along) << FINE, length)),
                )
            }
        };
        let place = |num: i128| {
            Num::from_bits(i64::try_from(num).expect("a push is shorter than a box and a body"))
        };
        let ground = add(moved, flat(centre));
        Some(Vec3::new(place(ground[0]), at.y, place(ground[1])))
    }

    /// Whether the insides of the box at `centre` and `other` at `other_centre` share a point:
    /// touching is not overlap.
    pub(crate) fn overlaps(
        &self,
        centre: Position,
        other: &BodyBox,
        other_centre: Position,
    ) -> bool {
        let apart = sub(flat(other_centre), flat(centre));
        self.frame().meets(apart, other.frame(), true)
    }

    /// How the distance between the box at `centre` and `other` at `other_centre` lies against
    /// `reach`; 0 when they touch or overlap.
    pub(crate) fn nearest_box(
        &self,
        centre: Position,
        other: &BodyBox,
        other_centre: Position,
        reach: Num,
    ) -> Ordering {
        let (ours, theirs) = (self.frame(), other.frame());
        let apart = sub(flat(other_centre), flat(centre));
        let reach = SquaredDistance::of(reach);
        if ours.meets(apart, theirs, false) {
            return SquaredDistance::ZERO.cmp(&reach);
        }
        // Apart, two convex shapes come nearest at a corner of one.
        let from_ours = ours
            .corners()
            .map(|corner| theirs.nearest_to(sub(corner, apart)).distance);
        let from_theirs = theirs
            .corners()
            .map(|corner| ours.nearest_to(add(corner, apart)).distance);
        let nearest = from_ours
            .into_iter()
            .chain(from_theirs)
            .min()
            .expect("eight corners");
        nearest.cmp(&reach)
    }

    /// Whether the insides of the box at `centre` and `polygon` share a point: an edge of the
    /// polygon passes through the open box, or the box lies inside the polygon, which its
    /// centre then tells. Touching is not overlap.
    pub(crate) fn overlaps_polygon(&self, centre: Position, polygon: &Polygon) -> bool {
        let frame = self.frame();
        let points = polygon.points();
        let crosses = (0..points.len()).any(|at| {
            let next = points[(at + 1) % points.len()];
            let start = sub(bits(points[at]), flat(centre));
            let path = sub(bits(next), bits(points[at]));
            frame.passes_through(start, path)
        });
        let at = centre.get();
        crosses || polygon.holds_point([at.x, at.z])
    }

    /// Whether a point `off`, from the box's position in halves of a bit, comes closer than
    /// `reach` to the box, exactly: as a grid's cell centres, whole only in halves, see it.
    pub(crate) fn closer_twice(&self, off: [i128; 2], reach: Num) -> bool {
        let reach = reach
            .checked_mul_int(2)
            .expect("a reach within a body and a walker doubles");
        self.frame_twice().nearest_to(off).distance < SquaredDistance::of(reach)
    }

    /// Whether the insides of the box and of a square share a point: the square's centre `off`
    /// from the box's position, and its half side `half`, both in halves of a bit, as a grid's
    /// cells lie. Touching is not overlap.
    pub(crate) fn overlaps_square_twice(&self, off: [i128; 2], half: i128) -> bool {
        let square = Frame {
            halves: [[half, 0], [0, half]],
        };
        self.frame_twice().meets(off, square, true)
    }

    fn frame(&self) -> Frame {
        Frame {
            halves: self.half.map(bits),
        }
    }

    /// The box at twice the scale, in halves of a bit.
    fn frame_twice(&self) -> Frame {
        Frame {
            halves: self.half.map(|edge| bits(edge).map(|axis| 2 * axis)),
        }
    }
}

/// A snapshot is untrusted, so half edges that lie flat, turn clockwise, or reach past 64 m fail
/// to decode; the bound is derived again.
impl<'de> Deserialize<'de> for BodyBox {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<BodyBox, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            half: [[Num; 2]; 2],
        }
        let Fields { half } = Fields::deserialize(deserializer)?;
        BodyBox::of_halves(half).ok_or_else(|| {
            D::Error::custom("a box's half edges turn counterclockwise, within 64 m")
        })
    }
}

impl Frame {
    /// The cross product of the half edges, positive: a quarter of the box's area.
    fn quarter_area(&self) -> i128 {
        let [a, b] = self.halves;
        cross(a, b)
    }

    /// The corners from the position, counterclockwise.
    fn corners(&self) -> [Flat; 4] {
        let [a, b] = self.halves;
        [sub([0, 0], add(a, b)), sub(a, b), add(a, b), sub(b, a)]
    }

    /// The two slab values of `off`, each `cross` of it with a half edge: it lies within the
    /// box when both are within `quarter_area` of 0.
    fn slabs(&self, off: Flat) -> [i128; 2] {
        let [a, b] = self.halves;
        [cross(off, b), cross(a, off)]
    }

    fn holds(&self, off: Flat) -> bool {
        let area = self.quarter_area();
        self.slabs(off).iter().all(|value| value.abs() <= area)
    }

    /// The nearest point's squared distance from `off`, and what of the box it is; the first
    /// edge on a tie.
    fn nearest_to(&self, off: Flat) -> Nearest {
        if self.holds(off) {
            return Nearest {
                distance: SquaredDistance::ZERO,
                feature: Feature::Inside,
            };
        }
        let corners = self.corners();
        let mut best: Option<Nearest> = None;
        for edge in 0..4 {
            let (start, end) = (corners[edge], corners[(edge + 1) % 4]);
            let along_edge = sub(end, start);
            let off_start = sub(off, start);
            let along = dot(off_start, along_edge);
            let length = dot(along_edge, along_edge);
            let found = if along <= 0 {
                Nearest {
                    distance: square(off_start),
                    feature: Feature::Corner(edge),
                }
            } else if along >= length {
                Nearest {
                    distance: square(sub(off, end)),
                    feature: Feature::Corner((edge + 1) % 4),
                }
            } else {
                let across = cross(along_edge, off_start).unsigned_abs();
                let square = U256::product(across, across);
                Nearest {
                    distance: SquaredDistance::new(square, length.unsigned_abs()),
                    feature: Feature::Edge(edge),
                }
            };
            if best.is_none_or(|best| found.distance < best.distance) {
                best = Some(found);
            }
        }
        best.expect("four edges")
    }

    /// The edge nearest `off`, which lies inside the box; the first on a tie.
    fn nearest_edge_inside(&self, off: Flat) -> usize {
        let corners = self.corners();
        let depth = |edge: usize| {
            let normal = self.normal(edge);
            let inward = dot(normal, sub(off, corners[edge])).unsigned_abs();
            SquaredDistance::new(
                U256::product(inward, inward),
                dot(normal, normal).unsigned_abs(),
            )
        };
        (0..4).min_by_key(|&edge| depth(edge)).expect("four edges")
    }

    /// The outward normal of `edge`, as long as the edge.
    fn normal(&self, edge: usize) -> Flat {
        let corners = self.corners();
        let along = sub(corners[(edge + 1) % 4], corners[edge]);
        [along[1], -along[0]]
    }

    /// Where `off` goes along `edge`'s outward normal to leave a body of `radius` touching the
    /// edge's line from outside: its foot on the line, exact before its one rounding, then
    /// `radius` out along the normal, its length a fine root. Each rounds to the nearest bit, so
    /// the end lies within a bit and a half of touching.
    fn out_through(&self, edge: usize, off: Flat, radius: Num) -> Flat {
        let normal = self.normal(edge);
        let out = dot(normal, sub(off, self.corners()[edge]));
        let square = dot(normal, normal);
        let length = fine_root(square.unsigned_abs());
        let radius = i128::from(radius.to_bits());
        [0, 1].map(|axis| {
            let along = normal[axis];
            off[axis] - divide(out * along, square) + divide((radius * along) << FINE, length)
        })
    }

    /// Whether the boxes' insides share a point, with `open`; or, without, whether they touch
    /// or overlap: no axis of either separates them. `other`'s position lies `apart` from this
    /// one's.
    fn meets(&self, apart: Flat, other: Frame, open: bool) -> bool {
        !self.separates(apart, other, open) && !other.separates(sub([0, 0], apart), *self, open)
    }

    /// Whether one of this box's slab axes separates it from `other`, whose position lies
    /// `apart` from this one's.
    fn separates(&self, apart: Flat, other: Frame, open: bool) -> bool {
        let area = self.quarter_area();
        let [their_a, their_b] = other.halves;
        let centre = self.slabs(apart);
        let (a_spread, b_spread) = (self.slabs(their_a), self.slabs(their_b));
        (0..2).any(|slab| {
            let limit = area + a_spread[slab].abs() + b_spread[slab].abs();
            if open {
                centre[slab].abs() >= limit
            } else {
                centre[slab].abs() > limit
            }
        })
    }

    /// The share where the path from `start`, along `path`, first meets the closed box, or none.
    fn entry(&self, start: Flat, path: Flat) -> Option<Fraction> {
        let clip = self.clip(start, path, false)?;
        let enter = clip
            .low
            .map_or(Fraction::ZERO, |low| low.max(Fraction::ZERO));
        let exit = clip
            .high
            .map_or(Fraction::ONE, |high| high.min(Fraction::ONE));
        (enter <= exit).then_some(enter)
    }

    /// Whether the closed path from `start`, along `path`, has a point inside the open box.
    fn passes_through(&self, start: Flat, path: Flat) -> bool {
        let Some(Clip { low, high }) = self.clip(start, path, true) else {
            return false;
        };
        let ordered = match (low, high) {
            (Some(low), Some(high)) => low < high,
            _ => true,
        };
        ordered
            && low.is_none_or(|low| low < Fraction::ONE)
            && high.is_none_or(|high| high > Fraction::ZERO)
    }

    /// The values of `t` for which `start + t·path` lies within both slabs, `open` or closed;
    /// `None` when no value does.
    fn clip(&self, start: Flat, path: Flat, open: bool) -> Option<Clip> {
        let area = self.quarter_area();
        let (at_start, along) = (self.slabs(start), self.slabs(path));
        let mut clip = Clip {
            low: None,
            high: None,
        };
        for slab in 0..2 {
            match span(at_start[slab], along[slab], area, open) {
                Span::Empty => return None,
                Span::All => {}
                Span::Between { low, high } => {
                    clip.low = Some(clip.low.map_or(low, |held| held.max(low)));
                    clip.high = Some(clip.high.map_or(high, |held| held.min(high)));
                }
            }
        }
        Some(clip)
    }
}

/// The values of `t` for which `value + t·along` lies within `bound` of 0, `open` or closed.
fn span(value: i128, along: i128, bound: i128, open: bool) -> Span {
    if along == 0 {
        let within = if open {
            value.abs() < bound
        } else {
            value.abs() <= bound
        };
        return if within { Span::All } else { Span::Empty };
    }
    let below = Fraction::new(-bound - value, along);
    let above = Fraction::new(bound - value, along);
    if along > 0 {
        Span::Between {
            low: below,
            high: above,
        }
    } else {
        Span::Between {
            low: above,
            high: below,
        }
    }
}

fn flat(at: Position) -> Flat {
    flat_vec(at.get())
}

fn flat_vec(at: Vec3) -> Flat {
    bits([at.x, at.z])
}

fn bits(at: [Num; 2]) -> Flat {
    at.map(|num| i128::from(num.to_bits()))
}

fn add(a: Flat, b: Flat) -> Flat {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: Flat, b: Flat) -> Flat {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: Flat, b: Flat) -> i128 {
    a[0] * b[0] + a[1] * b[1]
}

/// Positive when `b` turns counterclockwise from `a`.
fn cross(a: Flat, b: Flat) -> i128 {
    a[0] * b[1] - a[1] * b[0]
}

fn square(off: Flat) -> SquaredDistance {
    SquaredDistance::whole(dot(off, off).unsigned_abs())
}

/// `num / den` for a positive `den`, rounded to nearest, half away from zero, as collision
/// rounds a push.
fn divide(num: i128, den: i128) -> i128 {
    (num.abs() + den / 2) / den * num.signum()
}

/// The bits below a bit that a fine root keeps.
const FINE: u32 = 31;

/// `√square × 2³¹`, rounded down, for a `square` below 2⁶⁶: a way within a box's reach, so a
/// unit move along it divides by its length with an error below 2⁻³¹ of the move, half a bit
/// for a move within 64 m.
fn fine_root(square: u128) -> i128 {
    debug_assert!(square < 1 << 66, "a way within 64 m and a box's edge");
    (square << (2 * FINE)).floor_root().cast_signed()
}

#[cfg(test)]
mod tests;
