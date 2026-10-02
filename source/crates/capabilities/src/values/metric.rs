use std::cmp::Ordering;

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

    /// The unit vector along `vector` in the map's metric, which on a planar map drops the
    /// height; `None` for a vector of no length there.
    pub(crate) fn direction(self, vector: Vec3) -> Option<Vec3> {
        match self {
            Metric::Planar => Vec3::new(vector.x, Num::ZERO, vector.z).normalized(),
            Metric::Spatial => vector.normalized(),
        }
    }

    /// The point `step` from `from` toward `to` in the map's metric, or the point there when it
    /// is at most `step` away: a planar map steps on the ground plane, at `from`'s height.
    pub(crate) fn step_toward(self, from: Position, to: Position, step: Num) -> Position {
        let to = match self {
            Metric::Planar => Vec3::new(to.get().x, from.get().y, to.get().z),
            Metric::Spatial => to.get(),
        };
        let stepped = from.get().step_toward(to, step);
        Position::new(stepped).expect("a step ends between two points within the bound")
    }

    /// Whether `b` is within `range` of `a`, exactly, with no square root.
    fn within(self, a: Position, b: Position, range: Num) -> bool {
        Vec3::ZERO.within(self.offset(a, b), range)
    }

    /// Whether `range` from the edge of a body of `from_radius` at `from` reaches the edge of a
    /// body of `to_radius` at `to`, exactly: the one rule of reach, which ranges, areas, auras,
    /// script queries and homing hits all decide by. A point is a body of radius 0. A reach past
    /// what a number holds is past every distance within the bound.
    pub(crate) fn reaches(
        self,
        from: Position,
        from_radius: Num,
        range: Num,
        to: Position,
        to_radius: Num,
    ) -> bool {
        debug_assert!(range >= Num::ZERO && from_radius >= Num::ZERO && to_radius >= Num::ZERO);
        let reach = range
            .checked_add(from_radius)
            .and_then(|reach| reach.checked_add(to_radius));
        reach.is_none_or(|reach| self.within(from, to, reach))
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
        let approach = Approach::of(self.offset(from, to), self.offset(from, at), reach);
        (approach.nearest != Ordering::Greater).then_some(approach.share)
    }
}

/// How near a straight path comes to a point: its nearest distance against a reach, and the share
/// of the path at its nearest point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Approach {
    pub(crate) nearest: Ordering,
    pub(crate) share: PathShare,
}

impl Approach {
    /// The approach of the path `path`, the offset from its start to its end, to the point `off`
    /// from its start, against `reach`, exactly and without a square root; a negative reach is
    /// nearer than any distance.
    pub(crate) fn of(path: Vec3, off: Vec3, reach: Num) -> Approach {
        let length = path.length_squared_bits();
        let raw = |v: Vec3| [v.x, v.y, v.z].map(|n| i128::from(n.to_bits()));
        let along: i128 = raw(off).iter().zip(raw(path)).map(|(a, b)| a * b).sum();
        let Some(reach) = (reach >= Num::ZERO).then(|| {
            let bits = u128::from(reach.to_bits().cast_unsigned());
            bits * bits
        }) else {
            let share = PathShare { along: 0, length };
            return Approach {
                nearest: Ordering::Greater,
                share,
            };
        };
        if length == 0 || along <= 0 {
            return Approach {
                nearest: off.length_squared_bits().cmp(&reach),
                share: PathShare { along: 0, length },
            };
        }
        let along = along.cast_unsigned();
        if along >= length {
            return Approach {
                nearest: (off - path).length_squared_bits().cmp(&reach),
                share: PathShare {
                    along: length,
                    length,
                },
            };
        }
        // The squared distance from the line, times the squared length: exact in 256 bits.
        let apart = U256::product(off.length_squared_bits(), length);
        let allowed = U256::product(reach, length).checked_add(U256::product(along, along));
        let allowed = allowed.expect("squares of offsets within the world's bound fit 256 bits");
        Approach {
            nearest: apart.cmp(&allowed),
            share: PathShare { along, length },
        }
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
    fn a_reach_runs_from_edge_to_edge() {
        let num = |value| Num::from_int(value).unwrap();
        let (from, to) = (at(0, 0, 0), at(10, 0, 0));
        // 10 m between centres: a range of 7 from a body of 1 to one of 2 reaches, 7 - ε not; a
        // point reaches as a body of 0.
        let cases = [
            (num(1), num(7), num(2), true),
            (num(1), num(7) - Num::EPSILON, num(2), false),
            (Num::ZERO, num(10), Num::ZERO, true),
            (Num::ZERO, num(8), num(2), true),
            (Num::ZERO, num(8) - Num::EPSILON, num(2), false),
            // A reach past what a number holds reaches all.
            (Num::MAX, Num::MAX, Num::ZERO, true),
        ];
        for (from_radius, range, to_radius, reaches) in cases {
            let met = Metric::Planar.reaches(from, from_radius, range, to, to_radius);
            assert_eq!(met, reaches, "{from_radius:?} + {range:?} + {to_radius:?}");
        }
        // 4 m up and 3 m along: 3 apart on a planar map, 5 on a spatial one.
        let above = at(3, 4, 0);
        assert!(Metric::Planar.reaches(from, Num::ZERO, num(3), above, Num::ZERO));
        assert!(!Metric::Spatial.reaches(from, Num::ZERO, num(4), above, Num::ZERO));
        assert!(Metric::Spatial.reaches(from, num(1), num(3), above, num(1)));
    }

    #[test]
    fn a_direction_and_a_step_keep_to_the_ground_on_a_planar_map() {
        let num = |value| Num::from_int(value).unwrap();
        let vector = |x, y, z| Vec3::new(num(x), num(y), num(z));
        // 3 along and 4 up: on the ground, straight along x; straight up has no ground direction.
        assert_eq!(
            Metric::Planar.direction(vector(3, 4, 0)),
            Some(vector(1, 0, 0))
        );
        assert_eq!(Metric::Planar.direction(vector(0, 5, 0)), None);
        assert_eq!(
            Metric::Spatial.direction(vector(0, 5, 0)),
            Some(vector(0, 1, 0))
        );
        // A step of 1 toward (3, 4, 0): along the ground at the height it starts at on a planar
        // map, along the line in space on a spatial one. A step past the end ends there, which on
        // a planar map is the point below or above at the start's height.
        let (from, to) = (at(0, 0, 0), at(3, 4, 0));
        assert_eq!(Metric::Planar.step_toward(from, to, num(1)), at(1, 0, 0));
        let spatial = Position::new(from.get().step_toward(to.get(), num(1))).unwrap();
        assert_eq!(Metric::Spatial.step_toward(from, to, num(1)), spatial);
        assert_eq!(Metric::Planar.step_toward(from, to, num(5)), at(3, 0, 0));
        assert_eq!(Metric::Spatial.step_toward(from, to, num(5)), to);
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
