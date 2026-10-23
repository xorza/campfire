use serde::{Deserialize, Serialize};

use crate::player_slot_plan::error::NotPlayerSlotPlan;

pub(crate) mod error;

/// How the server opens a player slot of a session: for a player who joins before the start, for a
/// bot the server plays, or open, for a player who may join later. The terms carry one for each
/// player_slot the session plays, so every delegation signs the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerSlotPlan {
    Player,
    Bot,
    Open,
}

impl PlayerSlotPlan {
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
impl TryFrom<u8> for PlayerSlotPlan {
    type Error = NotPlayerSlotPlan;

    fn try_from(code: u8) -> Result<Self, NotPlayerSlotPlan> {
        match code {
            0 => Ok(Self::Player),
            1 => Ok(Self::Bot),
            2 => Ok(Self::Open),
            _ => Err(NotPlayerSlotPlan(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::player_slot_plan::PlayerSlotPlan;
    use crate::player_slot_plan::error::NotPlayerSlotPlan;

    #[test]
    fn a_plan_reads_back_from_its_code_and_no_other_byte_is_one() {
        for plan in [
            PlayerSlotPlan::Player,
            PlayerSlotPlan::Bot,
            PlayerSlotPlan::Open,
        ] {
            assert_eq!(PlayerSlotPlan::try_from(plan.code()), Ok(plan));
        }
        assert_eq!(PlayerSlotPlan::try_from(3), Err(NotPlayerSlotPlan(3)));
        assert_eq!(
            PlayerSlotPlan::try_from(u8::MAX),
            Err(NotPlayerSlotPlan(u8::MAX))
        );
    }
}
