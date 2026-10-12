use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::{Mut, World};
use campfire_sim::{Capability, SimSet};

use crate::abilities::cast_spends::CastSpends;
use crate::actions::ActionsSet;
use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_times::SlotTimes;
use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::items::item_holds::ItemHolds;
use crate::items::items_api::ItemsApi;
use crate::items::shop_book::ShopBook;

pub(crate) mod inventory;
pub(crate) mod inventory_data;
pub(crate) mod item_book;
pub(crate) mod item_data;
pub(crate) mod item_holds;
pub(crate) mod item_id;
pub(crate) mod items_api;
pub(crate) mod shop;
pub(crate) mod shop_book;
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

impl Module for Items {
    const CAPABILITY: Option<Capability> = Some(Capability::Items);
    const NEEDS: &'static [Capability] = &[Capability::Stats];

    fn declare<D: Declarations>(d: &mut D) {
        d.callback::<CastSpends>((), Items::spend_use);
        d.component::<Inventory>();
        d.book::<ItemBook>();
        d.book::<ShopBook>();
        d.api(ItemsApi::register);
    }

    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, the
    /// inventories its units carry, as state, and the modifiers their items hold, as each tick
    /// starts once the trades are in, and in Resolve once a use may have spent an item.
    fn install(cx: &mut InstallCx<'_>) {
        let schedule = &mut *cx.schedule;
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
            let carried =
                caster.get_components_mut::<(&mut Inventory, &mut ActionSlots, &mut SlotTimes)>();
            if let Ok((mut inventory, mut slots, mut times)) = carried {
                inventory.spend_use(&book, (&mut *slots, &mut *times), slot);
            }
        });
    }
}

#[cfg(test)]
mod tests;
