use crate::delegation::Delegation;
use crate::player_slot_plan::PlayerSlotPlan;

/// How a player slot starts in a session's header: the delegation of the player who joined it
/// before the start, a bot, or open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerSlotStart {
    Player(Box<Delegation>),
    Bot,
    Open,
}

impl PlayerSlotStart {
    /// The start of the player of `delegation`.
    pub fn player(delegation: Delegation) -> Self {
        Self::Player(Box::new(delegation))
    }

    /// The plan this start keeps.
    pub const fn plan(&self) -> PlayerSlotPlan {
        match self {
            Self::Player(_) => PlayerSlotPlan::Player,
            Self::Bot => PlayerSlotPlan::Bot,
            Self::Open => PlayerSlotPlan::Open,
        }
    }
}
