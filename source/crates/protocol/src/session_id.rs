use std::fmt;
use std::str::FromStr;

use campfire_math::{NotHex, hex};

/// A session's id: the hash of its terms, see `SessionTerms::session_id`. Delegations and
/// chain-head signatures name it, so neither counts in another session, and both sign the terms.
/// It writes, and reads back, as 64 lowercase hex digits.
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

impl FromStr for SessionId {
    type Err = NotHex;

    fn from_str(text: &str) -> Result<SessionId, NotHex> {
        hex::decode(text).map(SessionId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_reads_back_what_it_writes() {
        let id = SessionId::new([0xa7; 32]);
        let written = id.to_string();
        assert_eq!(written, "a7".repeat(32));
        assert_eq!(written.parse(), Ok(id));
        assert_eq!("a7".parse::<SessionId>(), Err(NotHex));
    }
}
