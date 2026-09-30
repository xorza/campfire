use crate::units::script_limits::ScriptLimits;

/// The scripts a match runs: within `limits`, with a pool for each of its `players` slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchScripts {
    pub limits: ScriptLimits,
    pub players: u32,
}
