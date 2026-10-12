use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::host_rule::OnAny;
use crate::position::error::OutOfBounds;
use crate::sim_state::SimComponent;

pub(crate) mod error;

/// Where an entity is. Every coordinate stays within ±`BOUND`, so exact squared distances fit a
/// `u128` with room to spare.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Position(Vec3);

impl Position {
    /// 2²⁰ m.
    pub const BOUND: Num = Num::from_bits(1 << (20 + Num::FRAC_BITS));
    pub const ORIGIN: Position = Position(Vec3::ZERO);

    /// `None` when a coordinate is beyond `BOUND`.
    pub const fn new(point: Vec3) -> Option<Position> {
        let bound = Position::BOUND.to_bits().unsigned_abs();
        if point.x.to_bits().unsigned_abs() > bound
            || point.y.to_bits().unsigned_abs() > bound
            || point.z.to_bits().unsigned_abs() > bound
        {
            return None;
        }
        Some(Position(point))
    }

    pub const fn get(self) -> Vec3 {
        self.0
    }

    /// The offset from here to `to` on the ground plane: heights never count towards a range.
    pub const fn ground_offset(self, to: Position) -> Vec3 {
        let offset =
            to.0.checked_sub(self.0)
                .expect("two places within the bound are less than a Num apart");
        Vec3::new(offset.x, Num::ZERO, offset.z)
    }

    /// Whether `to` is within `radius` of here on the ground plane, exactly: the range test of a
    /// planar map.
    pub const fn within_ground(self, to: Position, radius: Num) -> bool {
        Vec3::ZERO.within(self.ground_offset(to), radius)
    }
}

impl SimComponent for Position {
    const NAME: &'static str = "sim.position";
    type Host = OnAny;

    // Its decode keeps it within the world's bound; the capability that knows the map's adds
    // that check.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

/// Exact: every `i16` lies within ±`BOUND`.
impl From<(i16, i16, i16)> for Position {
    fn from((x, y, z): (i16, i16, i16)) -> Position {
        Position(Vec3::from((x, y, z)))
    }
}

/// Exact: every `i16` lies within ±`BOUND`.
impl From<[i16; 3]> for Position {
    fn from(point: [i16; 3]) -> Position {
        Position(Vec3::from(point))
    }
}

impl TryFrom<Vec3> for Position {
    type Error = OutOfBounds;

    fn try_from(point: Vec3) -> Result<Position, OutOfBounds> {
        Position::new(point).ok_or(OutOfBounds::Beyond)
    }
}

/// A snapshot is untrusted, so a position out of bounds fails to decode.
impl<'de> Deserialize<'de> for Position {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Position, D::Error> {
        Position::new(Vec3::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("position beyond the world bound"))
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn coordinates_stay_within_the_bound() {
        let bound = Position::BOUND;
        let past = Position::BOUND + Num::EPSILON;
        for (at, inside) in [
            (Vec3::new(bound, -bound, bound), true),
            (Vec3::new(past, Num::ZERO, Num::ZERO), false),
            (Vec3::new(Num::ZERO, -past, Num::ZERO), false),
            (Vec3::new(Num::ZERO, Num::ZERO, past), false),
            (Vec3::new(Num::MIN, Num::ZERO, Num::ZERO), false),
        ] {
            assert_eq!(Position::new(at).is_some(), inside, "{at:?}");
            let decoded = Binary::decode::<Position>(&Binary::encode(&at));
            assert_eq!(decoded.ok(), Position::new(at), "{at:?}");
        }
        assert_eq!(Position::BOUND, Num::from_int(1 << 20).unwrap());
    }

    #[test]
    fn a_triple_of_i16_converts_exactly_and_a_vector_within_the_bound() {
        // Every i16 lies within ±2²⁰: the least and greatest convert to themselves.
        let least = Position::new(Vec3::new(Num::int(-32_768), Num::ZERO, Num::int(32_767)));
        assert_eq!(Some(Position::from((i16::MIN, 0, i16::MAX))), least);
        assert_eq!(Some(Position::from([i16::MIN, 0, i16::MAX])), least);
        // A vector converts up to the bound, both signs, and is refused one ulp past it.
        let bound = Position::BOUND;
        let past = bound + Num::EPSILON;
        let edge = Vec3::new(bound, -bound, Num::ZERO);
        assert_eq!(Position::try_from(edge).map(Position::get), Ok(edge));
        for beyond in [
            Vec3::new(past, Num::ZERO, Num::ZERO),
            Vec3::new(Num::ZERO, -past, Num::ZERO),
            Vec3::new(Num::ZERO, Num::ZERO, past),
        ] {
            assert_eq!(
                Position::try_from(beyond),
                Err(OutOfBounds::Beyond),
                "{beyond:?}"
            );
        }
    }

    #[test]
    fn reach_counts_the_ground_plane_only() {
        let at = |x, y, z| Position::new(Vec3::new(Num::int(x), Num::int(y), Num::int(z))).unwrap();
        // 3 m along x and 4 m along z: 5 m on the ground, whatever the heights.
        let (from, to) = (at(1, 0, 1), at(4, 9, 5));
        assert_eq!(
            from.ground_offset(to),
            Vec3::new(Num::int(3), Num::ZERO, Num::int(4))
        );
        let reaches = [
            Num::int(5) - Num::EPSILON,
            Num::int(5),
            Num::int(5) + Num::EPSILON,
        ]
        .map(|radius| from.within_ground(to, radius));
        assert_eq!(reaches, [false, true, true]);
        assert_eq!(Position::ORIGIN.get(), Vec3::ZERO);
    }
}
