use bevy_ecs::component::Component;
use campfire_math::{Num, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::sim_state::SimComponent;

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
    pub const fn new(at: Vec3) -> Option<Position> {
        let bound = Position::BOUND.to_bits().unsigned_abs();
        if at.x.to_bits().unsigned_abs() > bound
            || at.y.to_bits().unsigned_abs() > bound
            || at.z.to_bits().unsigned_abs() > bound
        {
            return None;
        }
        Some(Position(at))
    }

    pub const fn get(self) -> Vec3 {
        self.0
    }

    /// The offset from here to `to` on the ground plane: heights never count towards a range.
    pub fn ground_offset(self, to: Position) -> Vec3 {
        let offset = to.0 - self.0;
        Vec3::new(offset.x, Num::ZERO, offset.z)
    }

    /// Whether `to` is within `radius` of here on the ground plane, exactly: the range test of a
    /// planar map.
    pub fn within_ground(self, to: Position, radius: Num) -> bool {
        Vec3::ZERO.within(self.ground_offset(to), radius)
    }
}

impl SimComponent for Position {
    const NAME: &'static str = "sim.position";
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
            let decoded = postcard::from_bytes::<Position>(&postcard::to_allocvec(&at).unwrap());
            assert_eq!(decoded.ok(), Position::new(at), "{at:?}");
        }
        assert_eq!(Position::BOUND, Num::from_int(1 << 20).unwrap());
    }

    #[test]
    fn reach_counts_the_ground_plane_only() {
        let num = |value| Num::from_int(value).unwrap();
        let at = |x, y, z| Position::new(Vec3::new(num(x), num(y), num(z))).unwrap();
        // 3 m along x and 4 m along z: 5 m on the ground, whatever the heights.
        let (from, to) = (at(1, 0, 1), at(4, 9, 5));
        assert_eq!(from.ground_offset(to), Vec3::new(num(3), Num::ZERO, num(4)));
        let reaches = [num(5) - Num::EPSILON, num(5), num(5) + Num::EPSILON]
            .map(|radius| from.within_ground(to, radius));
        assert_eq!(reaches, [false, true, true]);
        assert_eq!(Position::ORIGIN.get(), Vec3::ZERO);
    }
}
