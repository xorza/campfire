// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_actions::{
    ActionProgress, ActionSlots, ActionsSet, CastSpends, SlotClaims, SlotTimes, SlotsMut, UseGroup,
};
use campfire_common::EnumAll;
use campfire_declare::{
    ByType, Declarations, InstallCx, Module, ModuleSet, NamedMessages, SetPlace,
};
use campfire_sim::{Capability, SimSet};
use campfire_units::BodyUnit;

mod error;
mod inventory;
mod inventory_data;
mod item_book;
mod item_books;
mod item_data;
mod item_holds;
mod item_id;
mod items_action;
mod items_api;
mod items_data;
mod items_rules;
mod shop;
mod shop_data;
mod shopping;
mod trade_orders;

pub use crate::error::{ItemError, ItemsDataError};
pub use crate::inventory::{CarrierSlots, Inventory, ItemStack, Purchase};
pub use crate::inventory_data::InventoryData;
pub use crate::item_book::{ItemBook, ItemSpec};
pub use crate::item_data::ItemData;
pub use crate::item_holds::ItemHolds;
pub use crate::item_id::ItemId;
pub use crate::items_action::ItemsAction;
pub use crate::items_api::ItemsApi;
pub use crate::items_data::ItemsData;
pub use crate::shop::Shop;
pub use crate::shop_data::ShopData;
pub use crate::shopping::Shopping;
pub use crate::trade_orders::TradeOrders;

/// The `items` capability: item types a mode package declares, the inventories unit types carry
/// them in, and the unit types that are shops.
#[derive(Debug)]
pub struct Items;

/// The steps of `items`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ItemsSet {
    /// The tick's orders buy, sell and swap items.
    Commands,
    /// Once the tick's trades are in: carried items push their grants.
    HoldAtInputs,
    /// The units list the items they want.
    Want,
}

impl ModuleSet for ItemsSet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        let inputs = || SetPlace::stage(SimSet::Inputs);
        match self {
            Self::Commands => SetPlace::at(ActionsSet::Trade),
            Self::HoldAtInputs => inputs()
                .after(ActionsSet::Order)
                .before(ActionsSet::HoldAtInputs),
            Self::Want => SetPlace::at(ActionsSet::Resolve),
        }
    }
}

impl Module for Items {
    const CAPABILITY: Option<Capability> = Some(Capability::Items);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Set = ItemsSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ItemsData>();
        d.section::<InventoryData>();
        d.section::<ShopData>();
        d.callback::<CastSpends>((), Self::spend_use);
        d.component::<Inventory>();
        d.component::<UseGroup>();
        d.require::<Inventory, UseGroup>();
        d.require::<UseGroup, Inventory>();
        d.book::<ItemBook>(ItemBook::build);
        d.book::<ByType<Shop>>(Shop::book);
        d.api(ItemsApi::register);
        d.book::<ByType<Inventory>>(Inventory::book);
        d.book::<ByType<UseGroup>>(Inventory::groups);
        d.extend::<ByType<ActionSlots>>(Self::inventory_slots);
        d.extend::<SlotClaims>(Self::claim_slots);
        d.extend::<NamedMessages>(Self::named_messages);
        d.rule(Self::rule);
        d.part::<BodyUnit, Inventory>(|at| at.of_type::<Inventory>().cloned());
        d.part::<BodyUnit, UseGroup>(|at| at.of_type::<UseGroup>().copied());
    }

    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, the
    /// inventories its units carry, as state, and the modifiers their items hold, as each tick
    /// starts once the trades are in, and in Resolve once a use may have spent an item. On the
    /// server, the tick's orders trade in Inputs, as a trade pays and refunds the server's
    /// resources; a client predicts no trade.
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        cx.add(ItemsSet::HoldAtInputs, ItemHolds::want_items);
        cx.add(ItemsSet::Want, ItemHolds::want_items);
        cx.server(ItemsSet::Commands, TradeOrders::trade_items);
    }
}

impl Items {
    /// Spends a use of the consumable in `slot` of the unit of `entity`, whose action resolved,
    /// when the slot is one of its inventory's.
    fn spend_use(world: &mut World, entity: Entity, slot: u8) {
        let book = world.resource::<ItemBook>().clone();
        let mut caster = world.entity_mut(entity);
        let carried = caster.get_components_mut::<(
            &mut Inventory,
            &UseGroup,
            &mut ActionSlots,
            &mut SlotTimes,
            &mut ActionProgress,
        )>();
        if let Ok((mut inventory, &group, mut slots, mut times, mut progress)) = carried {
            let carrier = CarrierSlots {
                unit: SlotsMut {
                    slots: &mut slots,
                    times: &mut times,
                    progress: &mut progress,
                },
                group,
            };
            inventory.spend_use(&book, carrier, slot);
        }
    }
}
