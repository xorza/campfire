use campfire_math::{Num, Rounding, Vec3};
use campfire_sim::Position;

use crate::body_box::BodyBox;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A map's bounds: the closed rectangle from `min` to `max` on the ground plane, each `[x, z]`.
/// No unit is ever outside them. A map's `[bounds]` writes them as `min` and `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Bounds {
    min: [Num; 2],
    max: [Num; 2],
}

impl Bounds {
    /// The whole world, within `Position::BOUND`: the bounds of a match without a map.
    pub const WORLD: Self = Self {
        min: [Num::from_bits(-Position::BOUND.to_bits()); 2],
        max: [Position::BOUND; 2],
    };

    /// The bounds from `min` to `max`; `None` unless `min` is below `max` on both axes and both
    /// are within the world's bound.
    pub const fn new(min: [Num; 2], max: [Num; 2]) -> Option<Self> {
        let bound = Position::BOUND.to_bits();
        let mut axis = 0;
        while axis < 2 {
            let (low, high) = (min[axis].to_bits(), max[axis].to_bits());
            if low >= high || low < -bound || high > bound {
                return None;
            }
            axis += 1;
        }
        Some(Self { min, max })
    }

    pub const fn min(self) -> [Num; 2] {
        self.min
    }

    pub const fn max(self) -> [Num; 2] {
        self.max
    }

    /// Whether `pos` is within the bounds on the ground plane, edges included, and within the
    /// world's bound in height, as every place is.
    pub const fn contains(self, pos: Vec3) -> bool {
        self.min[0].to_bits() <= pos.x.to_bits()
            && pos.x.to_bits() <= self.max[0].to_bits()
            && self.min[1].to_bits() <= pos.z.to_bits()
            && pos.z.to_bits() <= self.max[1].to_bits()
            && pos.y.to_bits().unsigned_abs() <= Position::BOUND.to_bits().unsigned_abs()
    }

    /// Whether the box `body` at `pos` lies within the bounds on the ground plane, its edges on
    /// theirs among them.
    pub fn holds_box(self, pos: Vec3, body: &BodyBox) -> bool {
        let extent = body.extent();
        let ground = [pos.x, pos.z];
        (0..2).all(|axis| {
            ground[axis]
                .checked_sub(extent[axis])
                .is_some_and(|low| low >= self.min[axis])
                && ground[axis]
                    .checked_add(extent[axis])
                    .is_some_and(|high| high <= self.max[axis])
        })
    }

    /// How far a straight path from `from` along the unit vector `direction` stays within the
    /// bounds on the ground plane, and within the world's bound in height: 0 from a point
    /// outside. Each axis's distance rounds toward the start, so the path that far reaches no
    /// edge's far side. An axis the path barely moves along cannot be the one it leaves by, so a
    /// ratio too large to hold is never the nearest.
    pub fn exit(self, from: Vec3, direction: Vec3) -> Num {
        if !self.contains(from) {
            return Num::ZERO;
        }
        let height = Position::BOUND;
        [
            (from.x, direction.x, self.min[0], self.max[0]),
            (from.y, direction.y, -height, height),
            (from.z, direction.z, self.min[1], self.max[1]),
        ]
        .into_iter()
        .filter_map(|(at, moves, min, max)| {
            let edge = if moves > Num::ZERO { max } else { min };
            (edge - at).checked_div(moves, Rounding::Floor)
        })
        .min()
        .expect("a unit vector moves along an axis")
        .max(Num::ZERO)
    }

    /// The point of the bounds nearest `[x, z]`.
    pub fn clamp_ground(self, [x, z]: [Num; 2]) -> [Num; 2] {
        [
            x.clamp(self.min[0], self.max[0]),
            z.clamp(self.min[1], self.max[1]),
        ]
    }

    /// The point of the bounds nearest `pos` on the ground plane, at its height.
    pub fn clamp(self, pos: Vec3) -> Vec3 {
        self.ground_point([pos.x, pos.z], pos)
    }

    /// The point of the bounds nearest `[x, z]` on the ground plane, at the height of `pos`: a
    /// unit ordered to a point on the ground keeps its height.
    pub fn ground_point(self, ground: [Num; 2], pos: Vec3) -> Vec3 {
        let [x, z] = self.clamp_ground(ground);
        Vec3::new(x, pos.y, z)
    }
}

/// A map's `[bounds]`: `min` and `max` as `[x, z]`, refused unless they make bounds.
impl<'de> Deserialize<'de> for Bounds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: [Num; 2],
            max: [Num; 2],
        }
        let Fields { min, max } = Fields::deserialize(deserializer)?;
        Self::new(min, max).ok_or_else(|| {
            D::Error::custom("bounds need min below max, both within the world's bound")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_hold_their_closed_rectangle_and_clamp_to_it() {
        let bounds = Bounds::new([Num::int(-2), Num::int(-1)], [Num::int(4), Num::int(3)]).unwrap();
        let e = Num::EPSILON;
        // Corners and edges are inside; a bit past any edge is not. Height counts only past the
        // world's bound: 50 m up is inside, a bit past 2²⁰ m, up or down, is not.
        for (x, z, inside) in [
            (Num::int(-2), Num::int(-1), true),
            (Num::int(4), Num::int(3), true),
            (Num::int(4), Num::int(0), true),
            (Num::int(4) + e, Num::int(0), false),
            (Num::int(-2) - e, Num::int(0), false),
            (Num::int(0), Num::int(3) + e, false),
            (Num::int(0), Num::int(-1) - e, false),
        ] {
            assert_eq!(
                bounds.contains(Vec3::from((x, 50, z))),
                inside,
                "{x:?} {z:?}"
            );
        }
        // (9, 9) clamps to the corner (4, 3), (−5, 1) to the left edge at z = 1, and a point
        // inside stays, each at its own height.
        assert_eq!(bounds.clamp(Vec3::from((9, 7, 9))), Vec3::from((4, 7, 3)));
        assert_eq!(bounds.clamp(Vec3::from((-5, 5, 1))), Vec3::from((-2, 5, 1)));
        assert_eq!(bounds.clamp(Vec3::from((1, 0, 2))), Vec3::from((1, 0, 2)));
        assert_eq!(
            bounds.clamp_ground([Num::int(-9), Num::int(-9)]),
            [Num::int(-2), Num::int(-1)]
        );
        assert_eq!(
            bounds.ground_point([Num::int(9), Num::int(-9)], Vec3::from((1, 6, 2))),
            Vec3::from((4, 6, -1))
        );

        let past = Position::BOUND + e;
        for (min, max) in [
            ([Num::int(0), Num::int(0)], [Num::int(0), Num::int(1)]),
            ([Num::int(1), Num::int(0)], [Num::int(0), Num::int(1)]),
            ([Num::int(0), Num::int(0)], [past, Num::int(1)]),
            ([Num::int(0), -past], [Num::int(1), Num::int(1)]),
        ] {
            assert_eq!(Bounds::new(min, max), None, "{min:?} {max:?}");
        }
        assert!(Bounds::WORLD.contains(Vec3::from((-Position::BOUND, 0, Position::BOUND))));
        // A box of 2 × 1 m, half extents of 1 and 0.5, lies within them from x = −1 to 3 and
        // z = −0.5 to 2.5, its edges on theirs; a bit past either end is out.
        let boxed = BodyBox::new([Num::int(2), Num::ONE], Num::ZERO).unwrap();
        let half = Num::HALF;
        for (x, z, held) in [
            (Num::int(-1), -half, true),
            (Num::int(3), Num::int(3) - half, true),
            (Num::int(3) + e, Num::ZERO, false),
            (Num::int(-1) - e, Num::ZERO, false),
            (Num::ZERO, Num::int(3) - half + e, false),
            (Num::ZERO, -half - e, false),
        ] {
            let pos = Vec3::new(x, Num::ZERO, z);
            assert_eq!(bounds.holds_box(pos, &boxed), held, "{x:?} {z:?}");
        }
        for height in [Position::BOUND, -Position::BOUND] {
            assert!(bounds.contains(Vec3::new(Num::ZERO, height, Num::ZERO)));
        }
        for height in [past, -past] {
            assert!(!bounds.contains(Vec3::new(Num::ZERO, height, Num::ZERO)));
        }

        // From (0, 0, 1): 4 to the right edge along +x, 2 to the bottom along −z, and from the
        // right edge on along +x, 0. From a point past the right edge, 0, also back along −x and
        // straight up, which no ground axis bounds. Straight up, the path leaves at the world's
        // height.
        let x = Vec3::new(Num::ONE, Num::ZERO, Num::ZERO);
        let z = Vec3::new(Num::ZERO, Num::ZERO, -Num::ONE);
        let up = Vec3::new(Num::ZERO, Num::ONE, Num::ZERO);
        let origin = Vec3::from((0, 0, 1));
        assert_eq!(bounds.exit(origin, x), Num::int(4));
        assert_eq!(bounds.exit(origin, z), Num::int(2));
        assert_eq!(bounds.exit(Vec3::from((4, 0, 1)), x), Num::ZERO);
        let past = Vec3::from((5, 0, 1));
        for direction in [x, -x, up] {
            assert_eq!(bounds.exit(past, direction), Num::ZERO, "{direction:?}");
        }
        assert_eq!(bounds.exit(origin, up), Position::BOUND);
        // Along (0.6, 0, 0.8), each rounded once, the right edge lies 3 / 0.600 000 02 =
        // 4.999 999 8 m on, 3 · 2⁴⁸ / 10 066 330 = 83 886 076.67 bits, rounded down, as the
        // nearest, 83 886 077, would carry the path a hair past it: the distance times the
        // step's x stays within the 3 m.
        let slant = Vec3::new(
            Num::from_bits(10_066_330),
            Num::ZERO,
            Num::from_bits(13_421_773),
        );
        let from = Vec3::from((1, 0, -1));
        let exit = bounds.exit(from, slant);
        assert_eq!(exit, Num::from_bits(83_886_076));
        let reached = i128::from(exit.to_bits()) * i128::from(slant.x.to_bits());
        assert!(reached <= 3 << 48);
        assert!(reached + i128::from(slant.x.to_bits()) > 3 << 48);
    }
}
