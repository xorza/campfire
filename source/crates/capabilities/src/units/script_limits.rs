use serde::Deserialize;

/// How many operations scripts may run: in one call, and in each pool's calls of one tick
/// together. Each class of hook draws from its own pool, so AI cannot spend what a player's
/// cast needs, and the sum of the pools is the most one tick's scripts run. The mode's manifest
/// sets them, and its load checks that each pool holds a whole call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptLimits {
    pub per_call: u64,
    /// For the calls players cause: casts and mode inputs. It holds a whole call for each
    /// player.
    pub input: u64,
    /// For AI `think` calls.
    pub think: u64,
    /// For the mode's own calls: the match start and timers.
    pub mode: u64,
}
