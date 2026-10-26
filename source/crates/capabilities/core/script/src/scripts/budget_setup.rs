use crate::scripts::script_limits::ScriptLimits;

/// The budgets a match's scripts draw from: each budget's limit, and how many player budgets, one
/// for each player slot. A server's role holds them; the budgets each tick spends start from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetSetup {
    pub limits: ScriptLimits,
    pub players: u32,
}
