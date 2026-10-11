use crate::state_types::data_kind::DataKind;
use crate::state_types::kinded::Kinded;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// The way a unit faces, in degrees from `x` towards `−z`, counter-clockwise seen from above:
/// its spawn's angle, a placed unit's or a build order's, taken modulo 360, as its box body turns
/// by it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Facing(Num);

/// A full turn, in a `Num`'s bits.
const FULL: i64 = 360 << Num::FRAC_BITS;

impl Facing {
    /// `angle` degrees, taken modulo 360.
    pub const fn of(angle: Num) -> Facing {
        Facing(Num::from_bits(angle.to_bits().rem_euclid(FULL)))
    }

    /// Its degrees, at least 0 and below 360.
    pub const fn degrees(self) -> Num {
        self.0
    }
}

impl SimComponent for Facing {
    const NAME: &'static str = "units.facing";

    fn check(&self, _: &World, _: Entity) -> bool {
        (0..FULL).contains(&self.0.to_bits())
    }
}

impl Kinded for Facing {
    const KIND: DataKind = DataKind::Unit;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_facing_is_its_angle_modulo_360() {
        let degrees = |angle: i32| Facing::of(Num::int(angle.into())).degrees();
        assert_eq!(degrees(0), Num::ZERO);
        assert_eq!(degrees(90), Num::int(90));
        assert_eq!(degrees(360), Num::ZERO);
        assert_eq!(degrees(-90), Num::int(270));
        assert_eq!(degrees(725), Num::int(5));
        // The least step below 0 is the greatest below 360.
        let least = Facing::of(Num::from_bits(-1)).degrees();
        assert_eq!(least.to_bits(), FULL - 1);
        let world = World::new();
        let entity = Entity::PLACEHOLDER;
        assert!(Facing::of(Num::from_bits(-1)).check(&world, entity));
        assert!(!Facing(Num::int(360)).check(&world, entity));
        assert!(!Facing(Num::from_bits(-1)).check(&world, entity));
    }
}
