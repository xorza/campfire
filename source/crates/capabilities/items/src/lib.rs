#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, CastSpends, SlotTimes, UseGroup};
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_sim::Capability;
use campfire_units::BodyUnit;

mod inventory;
mod inventory_data;
mod item_book;
mod item_data;
mod item_holds;
mod item_id;
mod items_action;
mod items_api;
mod shop;
mod shop_data;
mod shopping;
mod trade_orders;

pub use crate::inventory::{CarrierSlots, Inventory, ItemStack, Purchase};
pub use crate::inventory_data::InventoryData;
pub use crate::item_book::{ItemBook, ItemSpec};
pub use crate::item_data::ItemData;
pub use crate::item_holds::ItemHolds;
pub use crate::item_id::ItemId;
pub use crate::items_action::ItemsAction;
pub use crate::items_api::ItemsApi;
pub use crate::shop::Shop;
pub use crate::shop_data::ShopData;
pub use crate::shopping::Shopping;
pub use crate::trade_orders::TradeOrders;

/// The `items` capability: item types a mode package declares, the inventories unit types carry
/// them in, and the unit types that are shops.
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
    pub const ALL;
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
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<ItemsSet>();
        d.callback::<CastSpends>((), Items::spend_use);
        d.component::<Inventory>();
        d.component::<UseGroup>();
        d.require::<Inventory, UseGroup>();
        d.require::<UseGroup, Inventory>();
        d.book::<ItemBook>();
        d.book::<ByType<Shop>>();
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
        let scripts = cx.role().scripts();
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
