use campfire_sim::{Capability, Position, StableId};

use crate::scripts::effects::Effect;

/// An order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiOrder {
    /// Attack `target`, a living enemy.
    Attack { target: StableId },
    /// Drop the target, and walk the path again.
    FollowPath,
    /// Drop the target, and walk to `to` off any path.
    Move { to: Position },
    /// Drop the target, walk to the spawn place off any path, and take no order until there.
    Reset,
}

impl Effect for AiOrder {
    const CAPABILITY: Capability = Capability::Orders;
}
