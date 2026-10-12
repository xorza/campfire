use campfire_sim::Capability;

use crate::scripts::hook::Hook;
use crate::scripts::script_api::status::Status;

/// Whether the release calls a hook, and its owner: the capability whose module calls it and
/// records it, `None` for the core's and the mode's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    pub hook: Hook,
    pub status: Status,
    pub capability: Option<Capability>,
}
