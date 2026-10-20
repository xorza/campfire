use campfire_players::CapabilityDirective;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `items` tells its units to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ItemsDirective<R: OrderRefs = Ids> {
    /// Buy one `item` at the shop unit `shop`.
    Buy { item: R::Item, shop: R::Unit },
    /// Sell the stack in inventory slot `slot` at the shop unit `shop`.
    Sell { slot: u8, shop: R::Unit },
    /// Swap inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
}

impl Copy for ItemsDirective {}

impl CapabilityDirective for ItemsDirective {
    const CAPABILITY: Capability = Capability::Items;
}

impl<R: OrderRefs> ItemsDirective<R> {
    /// The same directive with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ItemsDirective<T>, M::Error> {
        Ok(match self {
            Self::Buy { item, shop } => ItemsDirective::Buy {
                item: refs.item(item)?,
                shop: refs.unit(shop)?,
            },
            Self::Sell { slot, shop } => ItemsDirective::Sell {
                slot,
                shop: refs.unit(shop)?,
            },
            Self::Swap { from, to } => ItemsDirective::Swap { from, to },
        })
    }
}
