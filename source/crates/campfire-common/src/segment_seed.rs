use serde::{Deserialize, Serialize};

use crate::secret::Secret;

/// The seed of one log segment. It stays secret until the segment is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SegmentSeed(Secret<32>);

impl SegmentSeed {
    pub const fn new(bytes: [u8; 32]) -> SegmentSeed {
        SegmentSeed(Secret::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}
