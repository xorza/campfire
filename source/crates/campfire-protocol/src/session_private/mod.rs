use serde::{Deserialize, Serialize};

use crate::seed_chain::SeedChain;
use crate::session_private::error::SessionPrivateError;
use crate::session_terms::SessionTerms;

pub(crate) mod error;

/// Starts every private record and states its version, so other bytes are refused at once.
const PRIVATE_TAG: &[u8] = b"campfire/session-private/v1";

/// What a server keeps of a session that no one else sees, written once before its first offer:
/// the seed chain, whose root reveals every segment's seed, and the session's terms, so a
/// restart restores the session it opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPrivate {
    pub seed_chain: SeedChain,
    pub terms: SessionTerms,
}

impl SessionPrivate {
    /// The file's bytes: the tag, then the record in postcard.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(PRIVATE_TAG);
        postcard::to_io(self, out).expect("postcard into a Vec cannot fail");
    }

    /// The record of a file's `bytes`; an error for bytes that `encode` did not write.
    pub fn decode(bytes: &[u8]) -> Result<SessionPrivate, SessionPrivateError> {
        let rest = bytes
            .strip_prefix(PRIVATE_TAG)
            .ok_or(SessionPrivateError::NotPrivate)?;
        let (private, rest) = postcard::take_from_bytes::<SessionPrivate>(rest)
            .map_err(SessionPrivateError::Malformed)?;
        if !rest.is_empty() {
            return Err(SessionPrivateError::Trailing);
        }
        Ok(private)
    }
}

#[cfg(test)]
mod tests;
