use campfire_codec::{Binary, TaggedError};
use serde::{Deserialize, Serialize};

use crate::seed_chain::SeedChain;

/// Starts every private record and states its version, so other bytes are refused at once.
const PRIVATE_TAG: &[u8] = b"campfire/session-private/v3";

/// What a server keeps of a session that no one else sees, written once before its first offer:
/// the seed chain, whose root reveals every segment's seed. The session's terms are its
/// journal's header alone, so a restart has one record of them
/// (08-sessions.md, The terms are the journal's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPrivate {
    pub seed_chain: SeedChain,
}

impl SessionPrivate {
    /// The most bytes a private record's file may hold: its tag and its seed chain's few dozen,
    /// with room to spare, so a file of another kind is refused before it is read whole.
    pub const MAX_FILE_LEN: usize = 1 << 10;

    /// The file's bytes: the tag, then the record in postcard.
    pub fn encode(&self, out: &mut Vec<u8>) {
        Binary::encode_tagged(PRIVATE_TAG, self, out);
    }

    /// The record of a file's `bytes`; an error for bytes that `encode` did not write.
    pub fn decode(bytes: &[u8]) -> Result<Self, TaggedError> {
        Binary::decode_tagged(PRIVATE_TAG, bytes)
    }
}

#[cfg(test)]
mod tests;
