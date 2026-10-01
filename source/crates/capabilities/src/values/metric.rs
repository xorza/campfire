use bevy_ecs::resource::Resource;
use campfire_math::{Num, U256, Vec3};
use campfire_sim::Position;
use serde::Deserialize;

/// How a map measures ranges, reach and sight: on the ground plane, as MOBAs and RTS games do, or
/// in 3D, as shooters and flight do. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    #[default]
    Planar,
    Spatial,
}

impl Metric {
    /// The offset from `a` to `b` that counts towards a distance: a planar map drops the height.
    pub(crate) fn offset(self, a: Position, b: Position) -> Vec3 {
        match self {
            Metric::Planar => a.ground_offset(b),
            Metric::Spatial => b.get() - a.get(),
        }
    }

    /// Whether `b` is within `range` of `a`, exactly, with no square root.
    pub(crate) fn within(self, a: Position, b: Position, range: Num) -> bool {
        Vec3::ZERO.within(self.offset(a, b), range)
    }

    /// Whether the straight path from `from` to `to` comes within `reach` of `at`, decided
    /// exactly and without a square root; and if it does, the share of the path at its point
    /// nearest `at`.
    pub(crate) fn meets(
        self,
        from: Position,
        to: Position,
        at: Position,
        reach: Num,
    ) -> Option<PathShare> {
        if reach < Num::ZERO {
            return None;
        }
        let path = self.offset(from, to);
        let off = self.offset(from, at);
        let length = path.length_squared_bits();
        let reach = {
            let bits = u128::from(reach.to_bits().cast_unsigned());
            bits * bits
        };
        let raw = |v: Vec3| [v.x, v.y, v.z].map(|n| i128::from(n.to_bits()));
        let along: i128 = raw(off).iter().zip(raw(path)).map(|(a, b)| a * b).sum();
        if length == 0 || along <= 0 {
            return (off.length_squared_bits() <= reach).then_some(PathShare { along: 0, length });
        }
        let along = along.cast_unsigned();
        if along >= length {
            let beyond = self.offset(to, at).length_squared_bits();
            return (beyond <= reach).then_some(PathShare {
                along: length,
                length,
            });
        }
        // The squared distance from the line, times the squared length: exact in 256 bits.
        let apart = U256::product(off.length_squared_bits(), length);
        let allowed = U256::product(reach, length).checked_add(U256::product(along, along));
        let allowed = allowed.expect("squares of offsets within the world's bound fit 256 bits");
        (apart <= allowed).then_some(PathShare { along, length })
    }
}

/// A point of a straight path, as the share `along / length` of it, both the squared length's
/// raw units; a path of no length has the share 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathShare {
    pub(crate) along: u128,
    pub(crate) length: u128,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i64, y: i64, z: i64) -> Position {
        let num = |value| Num::from_int(value).unwrap();
        Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
    }

    #[test]
    fn a_path_meets_a_point_within_reach_of_its_nearest_point() {
        let num = |value| Num::from_int(value).unwrap();
        let (from, to) = (at(0, 0, 0), at(10, 0, 0));
        let share = |share: Option<PathShare>| share.map(|share| (share.along, share.length));
        // 100 m² of path in raw units, 2⁴⁸ each.
        let length = 100_u128 << 48;
        // Beside the path 2 m off, at 4 m along: met within 2, the share 4/10, and not within
        // 2 less a bit. Behind the start and past the end, the ends count.
        let cases = [
            (at(4, 0, 2), num(2), Some((40_u128 << 48, length))),
            (at(4, 0, 2), num(2) - Num::EPSILON, None),
            (at(-1, 0, 0), num(1), Some((0, length))),
            (at(-2, 0, 0), num(1), None),
            (at(12, 0, 0), num(2), Some((length, length))),
            (at(12, 0, 0), num(1), None),
        ];
        for (point, reach, expected) in cases {
            let met = Metric::Planar.meets(from, to, point, reach);
            assert_eq!(share(met), expected, "{point:?} within {reach:?}");
        }
        // 3 m up: a planar map ignores the height, a spatial one counts it, √(4 + 9) > 3.
        let above = at(4, 3, 2);
        assert!(Metric::Planar.meets(from, to, above, num(2)).is_some());
        assert!(Metric::Spatial.meets(from, to, above, num(3)).is_none());
        assert!(Metric::Spatial.meets(from, to, above, num(4)).is_some());
        // A path of no length meets what is within reach of its point.
        let still = Metric::Planar.meets(from, from, at(0, 0, 1), num(1));
        assert_eq!(share(still), Some((0, 0)));
    }
}
