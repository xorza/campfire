use crate::scripts::hook::Hook;
use crate::scripts::script_api::status::Status;

/// Whether the release calls a hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    pub hook: Hook,
    pub status: Status,
}
