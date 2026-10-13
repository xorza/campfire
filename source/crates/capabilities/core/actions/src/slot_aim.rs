use serde::{Deserialize, Serialize};

use crate::action_target::ActionTarget;

/// The action in `slot`, and what it is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotAim {
    pub slot: u8,
    pub target: ActionTarget,
}
