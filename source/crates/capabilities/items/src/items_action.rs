use campfire_players::CapabilityAction;
use campfire_sim::{Capability, StableId};
use serde::{Deserialize, Serialize};

use crate::item_id::ItemId;

/// What an order of `items` tells its units to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemsAction {
    /// Buy one `item` at the shop unit `shop`.
    Buy { item: ItemId, shop: StableId },
    /// Sell the stack in inventory slot `slot` at the shop unit `shop`.
    Sell { slot: u8, shop: StableId },
    /// Swap inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
}

impl CapabilityAction for ItemsAction {
    const CAPABILITY: Capability = Capability::Items;
}
