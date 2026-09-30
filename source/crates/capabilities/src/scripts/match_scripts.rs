use std::rc::Rc;

use crate::scripts::script_limits::ScriptLimits;
use crate::values::declared_name::DeclaredName;

/// The scripts a match runs: within `limits`, with a pool for each of its `players` slots, and
/// dealing the `damage_kinds` the mode declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchScripts {
    pub limits: ScriptLimits,
    pub players: u32,
    pub damage_kinds: Rc<[DeclaredName]>,
}
