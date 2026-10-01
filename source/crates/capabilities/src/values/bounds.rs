use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;

/// A map's bounds: the closed rectangle from `min` to `max` on the ground plane, each `[x, z]`.
/// No unit is ever outside them.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    min: [Num; 2],
    max: [Num; 2],
}

impl Bounds {
    /// The whole world, within `Position::BOUND`: the bounds of a match without a map.
    pub const WORLD: Bounds = Bounds {
        min: [Num::from_bits(-Position::BOUND.to_bits()); 2],
        max: [Position::BOUND; 2],
    };

    /// The bounds of the match in `world`: its map's, or the world's in a match without a map.
    pub(crate) fn of(world: &World) -> Bounds {
        world
            .get_resource::<Bounds>()
            .copied()
            .unwrap_or(Bounds::WORLD)
    }

    /// The bounds from `min` to `max`; `None` unless `min` is below `max` on both axes and both
    /// are within the world's bound.
    pub(crate) const fn new(min: [Num; 2], max: [Num; 2]) -> Option<Bounds> {
        let bound = Position::BOUND.to_bits();
        let mut axis = 0;
        while axis < 2 {
            let (low, high) = (min[axis].to_bits(), max[axis].to_bits());
            if low >= high || low < -bound || high > bound {
                return None;
            }
            axis += 1;
        }
        Some(Bounds { min, max })
    }

    pub(crate) const fn min(self) -> [Num; 2] {
        self.min
    }

    pub(crate) const fn max(self) -> [Num; 2] {
        self.max
    }

    /// Whether `pos` is within the bounds on the ground plane, edges included.
    pub(crate) const fn contains(self, pos: Position) -> bool {
        let at = pos.get();
        self.min[0].to_bits() <= at.x.to_bits()
            && at.x.to_bits() <= self.max[0].to_bits()
            && self.min[1].to_bits() <= at.z.to_bits()
            && at.z.to_bits() <= self.max[1].to_bits()
    }

    /// How far a straight path from `from` along the unit vector `direction` stays within the
    /// bounds on the ground plane, and within the world's bound in height: 0 from a point
    /// outside. An axis the path barely moves along cannot be the one it leaves by, so a ratio
    /// too large to hold is never the nearest.
    pub(crate) fn exit(self, from: Position, direction: Vec3) -> Num {
        let at = from.get();
        let height = Position::BOUND;
        [
            (at.x, direction.x, self.min[0], self.max[0]),
            (at.y, direction.y, -height, height),
            (at.z, direction.z, self.min[1], self.max[1]),
        ]
        .into_iter()
        .filter_map(|(at, moves, min, max)| {
            let edge = if moves > Num::ZERO { max } else { min };
            (edge - at).checked_div(moves)
        })
        .min()
        .expect("a unit vector moves along an axis")
        .max(Num::ZERO)
    }

    /// The point of the bounds nearest `[x, z]`.
    pub(crate) fn clamp_ground(self, [x, z]: [Num; 2]) -> [Num; 2] {
        [
            x.clamp(self.min[0], self.max[0]),
            z.clamp(self.min[1], self.max[1]),
        ]
    }

    /// The point of the bounds nearest `pos` on the ground plane, at its height.
    pub(crate) fn clamp(self, pos: Position) -> Position {
        let at = pos.get();
        self.ground_point([at.x, at.z], pos)
    }

    /// The point of the bounds nearest `[x, z]` on the ground plane, at the height of `at`: a
    /// unit ordered to a point on the ground keeps its height.
    pub(crate) fn ground_point(self, ground: [Num; 2], at: Position) -> Position {
        let [x, z] = self.clamp_ground(ground);
        Position::new(Vec3::new(x, at.get().y, z)).expect("bounds are within the world's bound")
    }
}

/// A map's `[bounds]`: `min` and `max` as `[x, z]`, refused unless they make bounds.
impl<'de> Deserialize<'de> for Bounds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Bounds, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: [Scalar; 2],
            max: [Scalar; 2],
        }
        let fields = Fields::deserialize(deserializer)?;
        let num = |scalar: Scalar| {
            scalar
                .to_num()
                .ok_or_else(|| D::Error::custom("a bounds value beyond a Num"))
        };
        let min = [num(fields.min[0])?, num(fields.min[1])?];
        let max = [num(fields.max[0])?, num(fields.max[1])?];
        Bounds::new(min, max).ok_or_else(|| {
            D::Error::custom("bounds need min below max, both within the world's bound")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    fn at(x: Num, y: Num, z: Num) -> Position {
        Position::new(Vec3::new(x, y, z)).unwrap()
    }

    #[test]
    fn bounds_hold_their_closed_rectangle_and_clamp_to_it() {
        let bounds = Bounds::new([num(-2), num(-1)], [num(4), num(3)]).unwrap();
        let e = Num::EPSILON;
        // Corners and edges are inside; a bit past any edge is not. Height never counts.
        for (x, z, inside) in [
            (num(-2), num(-1), true),
            (num(4), num(3), true),
            (num(4), num(0), true),
            (num(4) + e, num(0), false),
            (num(-2) - e, num(0), false),
            (num(0), num(3) + e, false),
            (num(0), num(-1) - e, false),
        ] {
            assert_eq!(bounds.contains(at(x, num(50), z)), inside, "{x:?} {z:?}");
        }
        // (9, 9) clamps to the corner (4, 3), (−5, 1) to the left edge at z = 1, and a point
        // inside stays, each at its own height.
        assert_eq!(
            bounds.clamp(at(num(9), num(7), num(9))),
            at(num(4), num(7), num(3))
        );
        assert_eq!(
            bounds.clamp(at(num(-5), num(5), num(1))),
            at(num(-2), num(5), num(1))
        );
        assert_eq!(
            bounds.clamp(at(num(1), num(0), num(2))),
            at(num(1), num(0), num(2))
        );
        assert_eq!(bounds.clamp_ground([num(-9), num(-9)]), [num(-2), num(-1)]);
        assert_eq!(
            bounds.ground_point([num(9), num(-9)], at(num(1), num(6), num(2))),
            at(num(4), num(6), num(-1))
        );

        let past = Position::BOUND + e;
        for (min, max) in [
            ([num(0), num(0)], [num(0), num(1)]),
            ([num(1), num(0)], [num(0), num(1)]),
            ([num(0), num(0)], [past, num(1)]),
            ([num(0), -past], [num(1), num(1)]),
        ] {
            assert_eq!(Bounds::new(min, max), None, "{min:?} {max:?}");
        }
        assert!(Bounds::WORLD.contains(at(-Position::BOUND, num(0), Position::BOUND)));

        // From (0, 0, 1): 4 to the right edge along +x, 2 to the bottom along −z, and from a
        // point past the right edge, 0. Straight up, the path leaves at the world's height.
        let x = Vec3::new(Num::ONE, Num::ZERO, Num::ZERO);
        let z = Vec3::new(Num::ZERO, Num::ZERO, -Num::ONE);
        let up = Vec3::new(Num::ZERO, Num::ONE, Num::ZERO);
        let origin = at(num(0), num(0), num(1));
        assert_eq!(bounds.exit(origin, x), num(4));
        assert_eq!(bounds.exit(origin, z), num(2));
        assert_eq!(bounds.exit(at(num(5), num(0), num(1)), x), Num::ZERO);
        assert_eq!(bounds.exit(origin, up), Position::BOUND);
    }
}
