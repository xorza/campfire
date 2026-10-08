use serde::{Deserialize, Serialize};

use crate::actions::action_target::ActionTarget;

/// The action in `slot`, and what it is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SlotAim {
    pub(crate) slot: u8,
    pub(crate) target: ActionTarget,
}
