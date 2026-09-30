use serde::{Deserialize, Serialize};

/// A package's fingerprint as the session terms name it: the bytes `content` computes from the
/// package's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub const fn new(bytes: [u8; 32]) -> Fingerprint {
        Fingerprint(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
