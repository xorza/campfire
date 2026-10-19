use serde::{Deserialize, Serialize};

use crate::seat_plan::error::NotSeatPlan;

pub(crate) mod error;

/// How the server opens a seat of a session: for a player who joins before the start, for a bot
/// the server plays, or open, for a player who may join later. The terms carry one for each seat
/// the session plays, so every delegation signs the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeatPlan {
    Player,
    Bot,
    Open,
}

impl SeatPlan {
    /// The byte the session id hashes it as.
    pub const fn code(self) -> u8 {
        match self {
            Self::Player => 0,
            Self::Bot => 1,
            Self::Open => 2,
        }
    }
}

/// The plan `code` gives it.
impl TryFrom<u8> for SeatPlan {
    type Error = NotSeatPlan;

    fn try_from(code: u8) -> Result<Self, NotSeatPlan> {
        match code {
            0 => Ok(Self::Player),
            1 => Ok(Self::Bot),
            2 => Ok(Self::Open),
            _ => Err(NotSeatPlan(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::seat_plan::SeatPlan;
    use crate::seat_plan::error::NotSeatPlan;

    #[test]
    fn a_plan_reads_back_from_its_code_and_no_other_byte_is_one() {
        for plan in [SeatPlan::Player, SeatPlan::Bot, SeatPlan::Open] {
            assert_eq!(SeatPlan::try_from(plan.code()), Ok(plan));
        }
        assert_eq!(SeatPlan::try_from(3), Err(NotSeatPlan(3)));
        assert_eq!(SeatPlan::try_from(u8::MAX), Err(NotSeatPlan(u8::MAX)));
    }
}
