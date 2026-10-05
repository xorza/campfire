use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::StateRegistry;

use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;

pub(crate) mod inventory;
pub(crate) mod inventory_data;
pub(crate) mod item_book;
pub(crate) mod item_data;
pub(crate) mod item_id;
pub(crate) mod items_api;
pub(crate) mod shop;
pub(crate) mod shop_data;

/// The `items` capability: item types a mode package declares, the inventories its unit types
/// carry them in, and its shop.
#[derive(Debug)]
pub struct Items;

impl Items {
    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, and
    /// the inventories its units carry, as state.
    pub fn install(world: &mut World, _: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(ItemBook::default());
        registry.register_component::<Inventory>();
    }
}
