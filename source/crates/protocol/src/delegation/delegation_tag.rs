/// A tag of the delegation event that carries one of its terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            DelegationTag::SessionKey => "session_key",
            DelegationTag::ServerKey => "server_key",
            DelegationTag::SessionId => "session_id",
            DelegationTag::SeedContribution => "seed_contribution",
            DelegationTag::Expiration => "expiration",
        }
    }
}
