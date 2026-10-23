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
    /// Sell the carried stack `stack` at the shop unit `shop`.
    Sell { stack: R::Carried, shop: R::Unit },
    /// Move the carried stack `from` to inventory slot `to`, which swaps it with what `to` holds.
    Swap { from: R::Carried, to: u8 },
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
            Self::Sell { stack, shop } => ItemsDirective::Sell {
                stack: refs.carried(stack)?,
                shop: refs.unit(shop)?,
            },
            Self::Swap { from, to } => ItemsDirective::Swap {
                from: refs.carried(from)?,
                to,
            },
        })
    }
}
