use campfire_players::CapabilityAction;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `items` tells its units to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ItemsAction<R: OrderRefs = Ids> {
    /// Buy one `item` at the shop unit `shop`.
    Buy { item: R::Item, shop: R::Unit },
    /// Sell the stack in inventory slot `slot` at the shop unit `shop`.
    Sell { slot: u8, shop: R::Unit },
    /// Swap inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
}

impl Copy for ItemsAction {}

impl CapabilityAction for ItemsAction {
    const CAPABILITY: Capability = Capability::Items;
}

impl<R: OrderRefs> ItemsAction<R> {
    /// The same action with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ItemsAction<T>, M::Error> {
        Ok(match self {
            Self::Buy { item, shop } => ItemsAction::Buy {
                item: refs.item(item)?,
                shop: refs.unit(shop)?,
            },
            Self::Sell { slot, shop } => ItemsAction::Sell {
                slot,
                shop: refs.unit(shop)?,
            },
            Self::Swap { from, to } => ItemsAction::Swap { from, to },
        })
    }
}
