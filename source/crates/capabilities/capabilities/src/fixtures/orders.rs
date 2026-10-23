use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_math::Vec3;
use campfire_orders::Orders;
use campfire_sim::StableId;
use campfire_units::Owner;
use campfire_units::internals::Spawn;
use campfire_values::{Team, UnitType};

/// The test fixtures of `Orders` that need a crate above its own.
pub trait OrdersFixtures {
    /// A hero of `slot`, of the type `hero`.
    fn hero(
        world: &mut World,
        hero: UnitType,
        position: impl Into<Vec3>,
        team: Team,
        slot: u32,
    ) -> StableId;
}

impl OrdersFixtures for Orders {
    fn hero(
        world: &mut World,
        hero: UnitType,
        position: impl Into<Vec3>,
        team: Team,
        slot: u32,
    ) -> StableId {
        Spawn::of(hero, position)
            .team(team)
            .parts(Owner::new(PlayerSlot::new(slot)))
            .place(world)
    }
}
