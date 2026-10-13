use campfire_core::CapabilityAction;
use campfire_sim::Capability;
use serde::{Deserialize, Serialize};

use crate::item_id::ItemId;

/// What an order of `items` tells its units to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemsAction {
    /// Buy one `item` at the mode's shop.
    Buy { item: ItemId },
    /// Sell the stack in inventory slot `slot` at the mode's shop.
    Sell { slot: u8 },
    /// Swap inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
}

impl CapabilityAction for ItemsAction {
    const CAPABILITY: Capability = Capability::Items;
}
