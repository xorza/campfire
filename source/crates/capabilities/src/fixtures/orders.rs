use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_core::{Owner, Team, UnitType};
use campfire_mode::Mode;
use campfire_orders::Orders;
use campfire_sim::{Position, StableId};

/// The test fixtures of `Orders` that need a crate above its own.
pub trait OrdersFixtures {
    /// A hero of `slot`, of the type `hero`.
    fn hero(
        world: &mut World,
        hero: UnitType,
        pos: impl Into<Position>,
        team: Team,
        slot: u32,
    ) -> StableId;
}

impl OrdersFixtures for Orders {
    fn hero(
        world: &mut World,
        hero: UnitType,
        pos: impl Into<Position>,
        team: Team,
        slot: u32,
    ) -> StableId {
        Mode::spawn(world, hero, pos, team, Owner::new(PlayerSlot::new(slot)))
    }
}
