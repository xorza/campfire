use campfire_script::ScriptBudgets;

/// What a match is to its host.
#[derive(Debug, Clone)]
pub enum Role {
    /// A server's or a verifier's match, which computes the whole state and runs scripts within
    /// its budgets; a test of the rules alone gives none, and its match runs no scripts.
    Server(Option<ScriptBudgets>),
    /// A client's match, which predicts its own player's units and runs no scripts: it builds no
    /// system that computes the server's own state, which reaches it from the server (design 14,
    /// D12).
    Client,
}

impl Role {
    /// Whether the match runs scripts: a server's within its budgets.
    pub const fn scripts(&self) -> bool {
        matches!(self, Role::Server(Some(_)))
    }

    /// The budgets the match's scripts run within; none for a match that runs no scripts.
    pub const fn budgets(&self) -> Option<&ScriptBudgets> {
        match self {
            Role::Server(budgets) => budgets.as_ref(),
            Role::Client => None,
        }
    }
}
