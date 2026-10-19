use campfire_sim::Capability;

use crate::scripts::hook::Hook;
use crate::scripts::planned_capability::PlannedCapability;
use crate::scripts::status::Status;

/// Whether the release calls a hook, and its owner: the capability whose module calls it and
/// records it, `None` for the core's and the mode's, or the capability it is planned with, of no
/// module the release builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    pub hook: Hook,
    pub status: Status,
    pub capability: Option<Capability>,
    pub planned_in: Option<PlannedCapability>,
}
