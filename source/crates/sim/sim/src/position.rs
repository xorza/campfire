use std::ops::{Deref, DerefMut};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use serde::{Deserialize, Serialize};

use crate::host_rule::OnAny;
use crate::sim_state::SimComponent;

/// Where an entity is. It lies within the map's bounds, which lie within ±`BOUND`, so exact
/// squared distances between two places fit a `u128` with room to spare.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Position(Vec3);

impl Position {
    /// 2²⁰ m: the bound of every coordinate of a place, which the map's bounds lie within.
    pub const BOUND: Num = Num::from_bits(1 << (20 + Num::FRAC_BITS));

    pub const fn get(self) -> Vec3 {
        self.0
    }

    /// Whether each of its coordinates lies within ±`BOUND`.
    pub const fn within_bound(self) -> bool {
        let bound = Self::BOUND.to_bits().unsigned_abs();
        self.0.x.to_bits().unsigned_abs() <= bound
            && self.0.y.to_bits().unsigned_abs() <= bound
            && self.0.z.to_bits().unsigned_abs() <= bound
    }
}

/// A place reads as its vector, as Avian's `Position` does: it holds no rule of its own.
impl Deref for Position {
    type Target = Vec3;

    fn deref(&self) -> &Vec3 {
        &self.0
    }
}

impl DerefMut for Position {
    fn deref_mut(&mut self) -> &mut Vec3 {
        &mut self.0
    }
}

impl From<Vec3> for Position {
    fn from(point: Vec3) -> Self {
        Self(point)
    }
}

impl<X: Into<Num>, Y: Into<Num>, Z: Into<Num>> From<(X, Y, Z)> for Position {
    fn from(point: (X, Y, Z)) -> Self {
        Self(Vec3::from(point))
    }
}

impl<T: Into<Num>> From<[T; 3]> for Position {
    fn from(point: [T; 3]) -> Self {
        Self(Vec3::from(point))
    }
}

impl From<Position> for Vec3 {
    fn from(place: Position) -> Self {
        place.0
    }
}

impl SimComponent for Position {
    const NAME: &'static str = "sim.position";
    type Host = OnAny;

    // The capability that knows the map's bounds checks that it lies within them.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn a_place_lies_within_the_bound_up_to_it_and_encodes_as_its_vector() {
        let bound = Position::BOUND;
        let past = Position::BOUND + Num::EPSILON;
        for (at, within) in [
            (Vec3::new(bound, -bound, bound), true),
            (Vec3::new(past, Num::ZERO, Num::ZERO), false),
            (Vec3::new(Num::ZERO, -past, Num::ZERO), false),
            (Vec3::new(Num::ZERO, Num::ZERO, past), false),
            (Vec3::new(Num::MIN, Num::ZERO, Num::ZERO), false),
        ] {
            let place = Position::from(at);
            assert_eq!(place.within_bound(), within, "{at:?}");
            assert_eq!((place.get(), Vec3::from(place), *place), (at, at, at));
            assert_eq!(Binary::encode(&place), Binary::encode(&at));
            assert_eq!(Binary::decode::<Position>(&Binary::encode(&at)), Ok(place));
        }
        assert_eq!(Position::BOUND, Num::from_int(1 << 20).unwrap());
        // A triple or an array converts as its vector does.
        let point = Vec3::new(Num::int(2), Num::ZERO, Num::int(-3));
        assert_eq!(Position::from((2, 0, -3)).get(), point);
        assert_eq!(Position::from([2, 0, -3]).get(), point);
    }
}
