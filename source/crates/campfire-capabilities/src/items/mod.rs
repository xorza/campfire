use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::{Mut, World};
use campfire_sim::{SimSet, StateRegistry};

use crate::abilities::cast_spends::CastSpends;
use crate::actions::ActionsSet;
use crate::actions::action_slots::ActionSlots;
use crate::combat::CombatSet;
use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::items::item_holds::ItemHolds;

pub(crate) mod inventory;
pub(crate) mod inventory_data;
pub(crate) mod item_book;
pub(crate) mod item_data;
pub(crate) mod item_holds;
pub(crate) mod item_id;
pub(crate) mod items_api;
pub(crate) mod shop;
pub(crate) mod shop_data;

/// The `items` capability: item types a mode package declares, the inventories its unit types
/// carry them in, and its shop.
#[derive(Debug)]
pub struct Items;

/// The systems of `items`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ItemsSet {
    /// In `SimSet::Inputs`, once the tick's trades are in: carried items hold their modifiers.
    HoldAtInputs,
}

impl Items {
    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, the
    /// inventories its units carry, as state, and the modifiers their items hold, as each tick
    /// starts once the trades are in, and in Resolve once a use may have spent an item.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(ItemBook::default());
        world
            .get_resource_or_init::<CastSpends>()
            .register(Items::spend_use);
        registry.register_component::<Inventory>();
        schedule.add_systems((
            ItemHolds::hold_items
                .in_set(SimSet::Inputs)
                .in_set(ItemsSet::HoldAtInputs)
                .before(ActionsSet::HoldAtInputs),
            ItemHolds::hold_items
                .in_set(SimSet::Resolve)
                .after(CombatSet::Damage)
                .before(ActionsSet::HoldAtResolve),
        ));
    }
}

impl Items {
    /// Spends a use of the consumable in `slot` of the unit of `entity`, whose action resolved,
    /// when the slot is one of its inventory's.
    fn spend_use(world: &mut World, entity: Entity, slot: u8) {
        world.resource_scope(|world, book: Mut<'_, ItemBook>| {
            let mut caster = world.entity_mut(entity);
            let carried = caster.get_components_mut::<(&mut Inventory, &mut ActionSlots)>();
            if let Ok((mut inventory, mut slots)) = carried {
                inventory.spend_use(&book, &mut slots, slot);
            }
        });
    }
}

#[cfg(test)]
mod tests;
