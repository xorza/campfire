use crate::delegation::Delegation;
use crate::slot_plan::SlotPlan;

/// How a slot starts in a session's header: the delegation of the player who joined it before the
/// start, a bot, or open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotStart {
    Player(Box<Delegation>),
    Bot,
    Open,
}

impl SlotStart {
    /// The start of the player of `delegation`.
    pub fn player(delegation: Delegation) -> SlotStart {
        SlotStart::Player(Box::new(delegation))
    }

    /// The plan this start keeps.
    pub const fn plan(&self) -> SlotPlan {
        match self {
            SlotStart::Player(_) => SlotPlan::Player,
            SlotStart::Bot => SlotPlan::Bot,
            SlotStart::Open => SlotPlan::Open,
        }
    }
}
