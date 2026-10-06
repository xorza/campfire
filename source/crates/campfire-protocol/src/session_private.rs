use serde::{Deserialize, Serialize};

use crate::seed_chain::SeedChain;
use crate::session_terms::SessionTerms;

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
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = PRIVATE_TAG.to_vec();
        postcard::to_io(self, &mut bytes).expect("postcard into a Vec cannot fail");
        bytes
    }
}
