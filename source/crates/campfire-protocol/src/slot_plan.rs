use serde::{Deserialize, Serialize};

/// How the server opens a slot of a session: for a player who joins before the start, for a bot
/// the server plays, or open, for a player who may join later. The terms carry one for each slot
/// the session plays, so every delegation signs the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotPlan {
    Player,
    Bot,
    Open,
}

impl SlotPlan {
    /// The byte the session id hashes it as.
    pub const fn code(self) -> u8 {
        match self {
            SlotPlan::Player => 0,
            SlotPlan::Bot => 1,
            SlotPlan::Open => 2,
        }
    }
}
