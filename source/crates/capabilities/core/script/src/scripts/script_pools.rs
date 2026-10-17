use crate::scripts::script_limits::ScriptLimits;

/// The pools a match's scripts draw from: each pool's limit, and how many player pools, one for
/// each player slot. A server's role holds them; the budgets each tick spends start from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptPools {
    pub limits: ScriptLimits,
    pub players: u32,
}
