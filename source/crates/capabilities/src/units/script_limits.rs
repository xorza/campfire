/// How many operations scripts may run: in one call, and in each pool's calls of one tick
/// together. Each class of hook draws from its own pool, so AI cannot spend what a player's
/// cast needs, and the sum of the pools is the most one tick's scripts run. A pool holds at
/// least one whole call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLimits {
    pub per_call: u64,
    /// For the calls players cause: casts. A mode sizes it for a call of each player's.
    pub input: u64,
    /// For AI `think` calls.
    pub think: u64,
}
