use campfire_sim::Capability;
use serde::{Deserialize, Serialize};

use crate::players::capability_action::CapabilityAction;
use crate::production::rally_target::RallyTarget;

/// What an order of `production` tells its units to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductionAction {
    /// Cancel the entry at `place` of a producer's train queue, the head 0.
    CancelTrain { place: u8 },
    /// Set where a producer's trained units go, or clear it with `None`.
    Rally { target: Option<RallyTarget> },
    /// Cancel a site of the player's building.
    CancelBuild,
}

impl CapabilityAction for ProductionAction {
    const CAPABILITY: Capability = Capability::Production;
}
