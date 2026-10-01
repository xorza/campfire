use campfire_sim::{Position, StableId};

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
