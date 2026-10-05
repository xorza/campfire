use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::StateRegistry;

pub(crate) mod inventory_data;
pub(crate) mod item_data;
pub(crate) mod items_api;
pub(crate) mod shop_data;

/// The `items` capability: item types a mode package declares, the inventories its unit types
/// carry them in, and its shop.
#[derive(Debug)]
pub struct Items;

impl Items {
    /// Adds items to a match, on stats. Its item types, inventories and shop are data its load
    /// checks; nothing runs in a tick yet.
    pub fn install(_: &mut World, _: &mut Schedule, _: &mut StateRegistry) {}
}
