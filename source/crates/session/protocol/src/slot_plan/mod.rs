use serde::{Deserialize, Serialize};

use crate::slot_plan::error::NotSlotPlan;

pub(crate) mod error;

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
            Self::Player => 0,
            Self::Bot => 1,
            Self::Open => 2,
        }
    }
}

/// The plan `code` gives it.
impl TryFrom<u8> for SlotPlan {
    type Error = NotSlotPlan;

    fn try_from(code: u8) -> Result<Self, NotSlotPlan> {
        match code {
            0 => Ok(Self::Player),
            1 => Ok(Self::Bot),
            2 => Ok(Self::Open),
            _ => Err(NotSlotPlan(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::slot_plan::SlotPlan;
    use crate::slot_plan::error::NotSlotPlan;

    #[test]
    fn a_plan_reads_back_from_its_code_and_no_other_byte_is_one() {
        for plan in [SlotPlan::Player, SlotPlan::Bot, SlotPlan::Open] {
            assert_eq!(SlotPlan::try_from(plan.code()), Ok(plan));
        }
        assert_eq!(SlotPlan::try_from(3), Err(NotSlotPlan(3)));
        assert_eq!(SlotPlan::try_from(u8::MAX), Err(NotSlotPlan(u8::MAX)));
    }
}
