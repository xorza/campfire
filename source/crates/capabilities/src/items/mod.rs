use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_sim::Capability;

use crate::actions::action_slots::ActionSlots;
use crate::actions::cast_spends::CastSpends;
use crate::actions::slot_times::SlotTimes;
use crate::actions::use_group::UseGroup;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::phase::Phase;
use crate::declare::set_place::SetPlace;
use crate::items::inventory::{CarrierSlots, Inventory};
use crate::items::item_book::ItemBook;
use crate::items::item_holds::ItemHolds;
use crate::items::items_api::ItemsApi;
use crate::items::shop_book::ShopBook;
use crate::items::trade_orders::TradeOrders;
use crate::units::body_unit::BodyUnit;
use crate::units::by_type::ByType;

pub(crate) mod inventory;
pub(crate) mod inventory_data;
pub(crate) mod item_book;
pub(crate) mod item_data;
pub(crate) mod item_holds;
pub(crate) mod item_id;
pub(crate) mod items_action;
pub(crate) mod items_api;
pub(crate) mod shop;
pub(crate) mod shop_book;
pub(crate) mod shop_data;
pub(crate) mod trade_orders;

/// The `items` capability: item types a mode package declares, the inventories its unit types
/// carry them in, and its shop.
#[derive(Debug)]
pub struct Items;

campfire_common::enum_all! {
    /// The steps of `items`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum ItemsSet {
        /// The tick's orders buy, sell and swap items.
        Commands,
        /// Once the tick's trades are in: carried items push their grants.
        HoldAtInputs,
        /// The units list the items they want.
        Want,
    }
    /// Every step.
    pub(crate) const ALL;
}

impl ModuleSet for ItemsSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Items);
    const ALL: &'static [ItemsSet] = &ItemsSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            ItemsSet::Commands => Phase::Trade,
            ItemsSet::HoldAtInputs => Phase::HoldItems,
            ItemsSet::Want => Phase::Resolve,
        })
    }
}

impl Module for Items {
    const CAPABILITY: Option<Capability> = Some(Capability::Items);
    const NEEDS: &'static [Capability] = &[Capability::Stats];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<ItemsSet>();
        d.callback::<CastSpends>((), Items::spend_use);
        d.component::<Inventory>();
        d.component::<UseGroup>();
        d.require::<Inventory, UseGroup>();
        d.require::<UseGroup, Inventory>();
        d.book::<ItemBook>();
        d.book::<ShopBook>();
        d.api(ItemsApi::register);
        d.book::<ByType<Inventory>>();
        d.book::<ByType<UseGroup>>();
        d.part::<BodyUnit, Inventory>(|at| at.of_type::<Inventory>().cloned());
        d.part::<BodyUnit, UseGroup>(|at| at.of_type::<UseGroup>().copied());
    }

    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, the
    /// inventories its units carry, as state, and the modifiers their items hold, as each tick
    /// starts once the trades are in, and in Resolve once a use may have spent an item. With the
    /// core's scripts, as on the server, the tick's orders trade in Inputs; a client predicts no
    /// trade.
    fn install(cx: &mut InstallCx<'_>) {
        let scripts = cx.scripts();
        let schedule = &mut *cx.schedule;
        schedule.add_systems((
            ItemHolds::want_items.in_set(ItemsSet::HoldAtInputs),
            ItemHolds::want_items.in_set(ItemsSet::Want),
        ));
        if scripts {
            schedule.add_systems(TradeOrders::trade_items.in_set(ItemsSet::Commands));
        }
    }
}

impl Items {
    /// Spends a use of the consumable in `slot` of the unit of `entity`, whose action resolved,
    /// when the slot is one of its inventory's.
    fn spend_use(world: &mut World, entity: Entity, slot: u8) {
        let book = world.resource::<ItemBook>().clone();
        let mut caster = world.entity_mut(entity);
        let carried = caster
            .get_components_mut::<(&mut Inventory, &UseGroup, &mut ActionSlots, &mut SlotTimes)>();
        if let Ok((mut inventory, &group, mut slots, mut times)) = carried {
            let carrier = CarrierSlots {
                slots: &mut slots,
                times: &mut times,
                group,
            };
            inventory.spend_use(&book, carrier, slot);
        }
    }
}

#[cfg(test)]
mod tests;
