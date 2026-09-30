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
}
