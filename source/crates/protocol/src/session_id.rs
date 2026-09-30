use std::fmt;

use crate::hex;

/// A session's id: the hash of its terms, see `SessionTerms::session_id`. Delegations and
/// chain-head signatures name it, so neither counts in another session, and both sign the terms.
/// It writes as 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId([u8; 32]);

impl SessionId {
    pub const fn new(bytes: [u8; 32]) -> SessionId {
        SessionId(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(&self.0))
    }
}
