use campfire_sim::StableId;

/// An order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiOrder {
    /// Attack `target`, a living enemy.
    Attack { target: StableId },
    /// Drop the target, and walk the path again.
    FollowPath,
}
