use campfire_sim::Capability;
use serde::{Deserialize, Serialize};

use crate::players::capability_action::CapabilityAction;

/// What an order of `progression` tells its units to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressionAction {
    /// Learn the next rank of the action in `slot`, for a point.
    Learn { slot: u8 },
}

impl CapabilityAction for ProgressionAction {
    const CAPABILITY: Capability = Capability::Progression;
}
