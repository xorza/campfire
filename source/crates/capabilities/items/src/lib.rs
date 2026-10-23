// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_actions::{
    ActionSlots, ActionsSystems, CastSpends, ProgressDues, SlotClaims, SlotsMut, UseGroup,
};
use campfire_common::EnumAll;
use campfire_declarations::{
    ById, Declarations, InstallCx, Module, ModuleSystems, NamedTexts, SetPlace,
};
use campfire_sim::{Capability, SimSystems, StableId};
use campfire_units::BodyUnit;

use crate::inventory::CarrierSlots;
use crate::shop_index::ShopIndex;
use campfire_values::UnitType;

mod error;
mod inventory;
mod inventory_data;
mod item_book;
mod item_books;
mod item_data;
mod item_holds;
mod items_api;
mod items_data;
mod items_directive;
mod items_rules;
mod shop;
mod shop_data;
mod shop_index;
mod shopping;
mod trade_orders;

pub use crate::error::{ItemError, ItemsDataError};
pub use crate::inventory::{Inventory, ItemStack, Purchase};
pub use crate::inventory_data::InventoryData;
pub use crate::item_book::{ItemBook, ItemSpec};
pub use crate::item_data::ItemData;
pub use crate::item_holds::ItemHolds;
pub use crate::items_api::ItemsApi;
pub use crate::items_data::ItemsData;
pub use crate::items_directive::ItemsDirective;
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
pub enum ItemsSystems {
    /// The tick's orders buy, sell and swap items.
    Commands,
    /// Once the tick's trades are in: carried items push their grants.
    HoldAtInputs,
    /// The units list the items they want.
    Want,
}

impl ModuleSystems for ItemsSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        let inputs = || SetPlace::stage(SimSystems::Inputs);
        match self {
            Self::Commands => SetPlace::at(ActionsSystems::Trade),
            Self::HoldAtInputs => inputs()
                .after(ActionsSystems::Order)
                .before(ActionsSystems::HoldAtInputs),
            Self::Want => SetPlace::at(ActionsSystems::Resolve),
        }
    }
}

impl Module for Items {
    const CAPABILITY: Option<Capability> = Some(Capability::Items);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[Capability::Vision];
    type Systems = ItemsSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ItemsData>();
        d.section::<InventoryData>();
        d.section::<ShopData>();
        d.callback::<CastSpends>(Capability::Items, Self::spend_use);
        d.component::<Inventory>();
        d.component::<UseGroup>();
        d.require::<Inventory, UseGroup>();
        d.require::<UseGroup, Inventory>();
        d.book::<ItemBook>(ItemBook::build);
        d.book::<ById<UnitType, Shop>>(Shop::book);
        d.cache::<ShopIndex>();
        d.observer(ShopIndex::typed);
        d.observer(ShopIndex::untyped);
        d.api(ItemsApi::register);
        d.book::<ById<UnitType, Inventory>>(Inventory::book);
        d.book::<ById<UnitType, UseGroup>>(Inventory::groups);
        d.extend::<ById<UnitType, ActionSlots>>(Self::inventory_slots);
        d.extend::<SlotClaims>(Self::claim_slots);
        d.extend::<NamedTexts>(Self::named_texts);
        d.rule(Self::rule);
        d.part::<BodyUnit, Inventory>(|at| at.of_type::<Inventory>().cloned());
        d.part::<BodyUnit, UseGroup>(|at| at.of_type::<UseGroup>().copied());
    }

    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, the
    /// inventories its units carry, as state, and the modifiers their items hold, as each tick
    /// starts once the trades are in, and in Resolve once a use may have spent an item. On the
    /// server, the tick's orders trade in Inputs, as a trade pays and refunds the server's
    /// resources; a client predicts no trade.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.add(ItemsSystems::HoldAtInputs, ItemHolds::want_items);
        cx.add(ItemsSystems::Want, ItemHolds::want_items);
        cx.server(ItemsSystems::Commands, TradeOrders::trade_items);
    }
}

impl Items {
    /// Spends a use of the consumable in `slot` of the unit of `entity`, whose action resolved,
    /// when the slot is one of its inventory's.
    fn spend_use(world: &mut World, entity: Entity, slot: u8) {
        let book = world.resource::<ItemBook>().clone();
        let Some(&id) = world.get::<StableId>(entity) else {
            return;
        };
        ProgressDues::with(world, id, entity, |world, dues| {
            let mut caster = world.entity_mut(entity);
            let carried = caster.get_components_mut::<(&mut Inventory, &UseGroup, SlotsMut)>();
            if let Ok((mut inventory, &group, unit)) = carried {
                let carrier = CarrierSlots {
                    unit,
                    dues: dues.reborrow(),
                    group,
                };
                inventory.spend_use(&book, carrier, slot);
            }
        });
    }
}
