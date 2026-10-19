use crate::delegation::Delegation;
use crate::seat_plan::SeatPlan;

/// How a seat starts in a session's header: the delegation of the player who joined it before the
/// start, a bot, or open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatStart {
    Player(Box<Delegation>),
    Bot,
    Open,
}

impl SeatStart {
    /// The start of the player of `delegation`.
    pub fn player(delegation: Delegation) -> Self {
        Self::Player(Box::new(delegation))
    }

    /// The plan this start keeps.
    pub const fn plan(&self) -> SeatPlan {
        match self {
            Self::Player(_) => SeatPlan::Player,
            Self::Bot => SeatPlan::Bot,
            Self::Open => SeatPlan::Open,
        }
    }
}
