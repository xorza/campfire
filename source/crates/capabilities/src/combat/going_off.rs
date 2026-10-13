use campfire_sim::StableId;

/// An attack that goes off this tick: its attacker and its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct GoingOff {
    pub(super) attacker: StableId,
    pub(super) target: StableId,
}
