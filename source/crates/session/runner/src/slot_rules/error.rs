use campfire_protocol::AfterLeave;
use thiserror::Error;

/// Why the mode's `[players]` refuses a change of a slot's controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SlotRuleError {
    /// A new player took an open slot, and the mode has no late join.
    #[error("a late join, which the mode does not allow")]
    LateJoin,
    /// A new player took a bot's slot, and the mode has no bot takeover.
    #[error("a bot's slot taken over, which the mode does not allow")]
    BotTakeover,
    /// A left player's slot became `becomes`, and the mode's `leaver` says otherwise.
    #[error("a leaver's slot became {becomes:?}, which the mode does not say")]
    Leaver { becomes: AfterLeave },
}
