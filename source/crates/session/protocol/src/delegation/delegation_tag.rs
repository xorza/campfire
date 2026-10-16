use serde::{Deserialize, Serialize};

/// A tag of the delegation event that carries one of its terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationTag {
    SessionKey,
    ServerKey,
    SessionId,
    SeedContribution,
    /// NIP-40's tag, in Unix seconds.
    Expiration,
}

impl DelegationTag {
    /// The tag's name, as the event writes it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SessionKey => "session_key",
            Self::ServerKey => "server_key",
            Self::SessionId => "session_id",
            Self::SeedContribution => "seed_contribution",
            Self::Expiration => "expiration",
        }
    }
}
