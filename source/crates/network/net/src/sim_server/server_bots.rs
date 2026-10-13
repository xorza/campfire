use campfire_common::PlayerSlot;

use crate::order_script::OrderScript;

/// The bots a server plays: a script for each slot its plan gives a bot, and one for each slot
/// that becomes a bot's after its player left, whose ticks count from then. A bot slot with no
/// script idles.
#[derive(Debug, Clone, Default)]
pub struct ServerBots {
    pub slots: Vec<SlotBot>,
    pub takeover: Option<OrderScript>,
}

/// A slot the plan gives a bot, and the script it plays from the start.
#[derive(Debug, Clone)]
pub struct SlotBot {
    pub slot: PlayerSlot,
    pub script: OrderScript,
}

impl SlotBot {
    /// The bot of the slot of index `slot`, which plays `script`.
    pub const fn new(slot: u32, script: OrderScript) -> SlotBot {
        SlotBot {
            slot: PlayerSlot::new(slot),
            script,
        }
    }
}
