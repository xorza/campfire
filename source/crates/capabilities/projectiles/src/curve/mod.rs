use campfire_math::{Num, Rounding, Vec3};

use crate::projectile_spec::CurveSpec;

/// The most points a curve flies: past `2²⁶ + 1`, the weights of a point's four control points
/// would pass 128 bits beside their coordinates, so a longer flight flies faster than its speed.
pub(crate) const MAX_POINTS: u32 = (1 << 26) + 1;

/// How deep the length's rule splits a curve: a part past it measures as it stands. Each split
/// quarters a part's excess of its control polygon over its chord, so no curve within the world
/// needs as many.
const MAX_SPLITS: u32 = 32;

/// A cubic Bézier curve in space by its four control points (20-zero-hour-projectiles.md, A
/// projectile may fly a curve).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Curve {
    points: [Vec3; 4],
}

impl Curve {
    /// The curve of `spec` from `start` to `end`, points in space, its inner points at its
    /// fractions of the way on the ground plane, each rounded once, and its heights above the
    /// highest of its ends and `highest`, the ground's between them, if any, as Zero Hour's
    /// `calcFlightPath` sets them.
    pub(crate) fn new(start: Vec3, end: Vec3, spec: &CurveSpec, highest: Option<Num>) -> Self {
        let top = start.y.max(end.y);
        let top = highest.map_or(top, |highest| top.max(highest));
        let inner = |[fraction, height]: [Num; 2]| {
            let along = |from: Num, to: Num| from + (to - from) * fraction;
            Vec3::new(along(start.x, end.x), top + height, along(start.z, end.z))
        };
        Self {
            points: [start, inner(spec.first), inner(spec.second), end],
        }
    }

    /// Its point `step` of `count`: its value at `t = step ÷ (count − 1)`, its first for a count
    /// of 1, each coordinate from the four control points by their Bernstein weights in one step
    /// and rounded once, to nearest, ties to even.
    pub(crate) fn point(&self, step: u32, count: u32) -> Vec3 {
        debug_assert!(
            (1..=MAX_POINTS).contains(&count) && step < count,
            "a point of a curve"
        );
        let last = i128::from(count - 1);
        if last == 0 {
            return self.points[0];
        }
        let [k, j] = [i128::from(step), last - i128::from(step)];
        // The weights sum to `last³`, at most 2⁷⁸, so each sum of a coordinate, within 2⁴⁵ bits,
        // stays within 2¹²³.
        let weights = [j * j * j, 3 * j * j * k, 3 * j * k * k, k * k * k];
        Self::combine(&self.points, weights, last * last * last)
    }

    /// Its length as `BezierSegment::getApproximateLength` measures it: half its chord plus half
    /// its control polygon, split at its middle while the polygon passes the chord by more than
    /// 1 m, each length to the nearest bit.
    pub(crate) fn length(&self) -> Num {
        Self::length_of(self.points, 0)
    }

    /// Its count of points at `speed` a tick: ⌈length ÷ speed⌉, at least 1 and at most
    /// `MAX_POINTS`.
    pub(crate) fn count(&self, speed: Num) -> u32 {
        debug_assert!(speed > Num::ZERO, "a projectile's speed is positive");
        let steps = Rounding::Ceiling.divide(
            i128::from(self.length().to_bits()),
            i128::from(speed.to_bits()),
        );
        u32::try_from(steps.clamp(1, MAX_POINTS.into())).expect("a count of points fits u32")
    }

    fn length_of(points: [Vec3; 4], splits: u32) -> Num {
        let [p0, p1, p2, p3] = points;
        let chord = (p3 - p0).length();
        let polygon = (p1 - p0).length() + (p2 - p1).length() + (p3 - p2).length();
        if polygon - chord > Num::ONE && splits < MAX_SPLITS {
            let [left, right] = Self::halves(points);
            return Self::length_of(left, splits + 1) + Self::length_of(right, splits + 1);
        }
        (chord + polygon) / 2
    }

    /// The two halves of the curve of `points`, split at its middle by de Casteljau's rule, each
    /// new point rounded once.
    fn halves(points: [Vec3; 4]) -> [[Vec3; 4]; 2] {
        let at = |weights: [i128; 4], divisor: i128| Self::combine(&points, weights, divisor);
        let middle = at([1, 3, 3, 1], 8);
        [
            [points[0], at([1, 1, 0, 0], 2), at([1, 2, 1, 0], 4), middle],
            [middle, at([0, 1, 2, 1], 4), at([0, 0, 1, 1], 2), points[3]],
        ]
    }

    /// The sum of `points` by `weights`, over `divisor`, each coordinate rounded once.
    fn combine(points: &[Vec3; 4], weights: [i128; 4], divisor: i128) -> Vec3 {
        let axis = |of: fn(&Vec3) -> Num| {
            let sum: i128 = points
                .iter()
                .zip(weights)
                .map(|(point, weight)| weight * i128::from(of(point).to_bits()))
                .sum();
            let bits = Rounding::NearestEven.divide(sum, divisor);
            Num::from_bits(i64::try_from(bits).expect("a point among its control points"))
        };
        Vec3::new(axis(|p| p.x), axis(|p| p.y), axis(|p| p.z))
    }
}

#[cfg(test)]
mod tests;
