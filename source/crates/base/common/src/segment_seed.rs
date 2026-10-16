use serde::{Deserialize, Serialize};

use crate::domain_hasher::HashOutput;
use crate::secret::Secret;

/// The seed of one log segment. It stays secret until the segment is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SegmentSeed(Secret<32>);

impl SegmentSeed {
    /// The seed of a match whose randomness nothing reads: a restore target, whose state comes
    /// from a snapshot, as the seed is not state, or a test's.
    pub const ZERO: Self = Self::new([0; 32]);

    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(Secret::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl HashOutput for SegmentSeed {
    fn of_hash(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }
}
