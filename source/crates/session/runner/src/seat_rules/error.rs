use campfire_common::AfterLeave;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why the mode's `[players]` refuses a change of a seat's controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum SeatRuleError {
    /// A new player took an open seat, and the mode has no late join.
    #[error("a late join, which the mode does not allow")]
    LateJoin,
    /// A new player took a bot's seat, and the mode has no bot takeover.
    #[error("a bot's seat taken over, which the mode does not allow")]
    BotTakeover,
    /// A left player's seat became `becomes`, and the mode's `leaver` says otherwise.
    #[error("a leaver's seat became {becomes:?}, which the mode does not say")]
    Leaver { becomes: AfterLeave },
}
