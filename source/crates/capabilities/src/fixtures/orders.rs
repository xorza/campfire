use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_math::Vec3;
use campfire_mode::Mode;
use campfire_orders::Orders;
use campfire_sim::StableId;
use campfire_units::Owner;
use campfire_values::{Team, UnitType};

/// The test fixtures of `Orders` that need a crate above its own.
pub trait OrdersFixtures {
    /// A hero of `slot`, of the type `hero`.
    fn hero(
        world: &mut World,
        hero: UnitType,
        pos: impl Into<Vec3>,
        team: Team,
        slot: u32,
    ) -> StableId;
}

impl OrdersFixtures for Orders {
    fn hero(
        world: &mut World,
        hero: UnitType,
        pos: impl Into<Vec3>,
        team: Team,
        slot: u32,
    ) -> StableId {
        Mode::spawn(world, hero, pos, team, Owner::new(PlayerSlot::new(slot)))
    }
}
