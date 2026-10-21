use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_navigation::Navigation;
use campfire_sim::StableId;
use campfire_units::internals::Spawn;
use campfire_units::{UnitTypeCore, Units};
use campfire_values::{TypeScope, UnitType};

use crate::fixtures::walker_spawn::WalkerSpawn;

/// The test fixtures of `Navigation` that need a crate above its own.
pub trait NavigationFixtures {
    /// A unit at `position` walking a meter a tick to `to`.
    fn unit(world: &mut World, position: impl Into<Vec3>, to: Option<Vec3>) -> StableId;

    /// The mode's type `walker`, of no section, which the navigation tests' units are of:
    /// loaded with the first call, which comes before any unit spawns, with the spawner of
    /// every type.
    fn walker_type(world: &mut World) -> UnitType;

    /// A unit of the mode's type `walker` at `position`, of team 0, facing 0 degrees, with no
    /// body, that does not walk, unless the `WalkerSpawn`'s setters say otherwise.
    fn walker_at(world: &mut World, position: impl Into<Vec3>) -> WalkerSpawn;
}

impl NavigationFixtures for Navigation {
    fn unit(world: &mut World, position: impl Into<Vec3>, to: Option<Vec3>) -> StableId {
        Self::walker_at(world, position)
            .walks(Num::ONE, to)
            .place(world)
    }

    fn walker_type(world: &mut World) -> UnitType {
        Units::type_named(world, "walker").unwrap_or_else(|| {
            let core = UnitTypeCore::default();
            Units::load_type(world, TypeScope::Mode, "walker", &core)
        })
    }

    fn walker_at(world: &mut World, position: impl Into<Vec3>) -> WalkerSpawn {
        let unit_type = Self::walker_type(world);
        WalkerSpawn::new(Spawn::of(unit_type, position))
    }
}
