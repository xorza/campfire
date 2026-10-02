use blake3::Hasher;
use serde::{Deserialize, Serialize};

/// Starts each link of the seed chain, so no other BLAKE3 use can produce one.
const CHAIN_DOMAIN: &[u8] = b"campfire/seed-chain/v1";

/// A segment's server seed, `s_k` of the session's `SeedChain`. It predicts every hidden random
/// outcome of its segment, so it stays secret until the segment is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSeed([u8; 32]);

/// `C(s_0)`, the hash of the first segment's server seed. The terms hold it, so the session id
/// binds the server to every segment's seed before any player joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeedCommitment([u8; 32]);

impl ServerSeed {
    pub const fn new(bytes: [u8; 32]) -> ServerSeed {
        ServerSeed(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Whether this is segment `segment`'s seed of the chain `commitment` commits to: `segment + 1`
    /// links lead from it to the commitment.
    pub fn check(&self, segment: u32, commitment: &SeedCommitment) -> bool {
        let mut seed = *self;
        for _ in 0..segment {
            seed = seed.earlier();
        }
        seed.commitment() == *commitment
    }

    /// `C(s_k) = s_(k−1)`, where `C(x) = BLAKE3(domain ‖ x)`.
    pub(crate) fn earlier(&self) -> ServerSeed {
        ServerSeed(self.link())
    }

    /// `C(s_0)`, when this is the first segment's seed.
    pub(crate) fn commitment(&self) -> SeedCommitment {
        SeedCommitment(self.link())
    }

    fn link(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(CHAIN_DOMAIN).update(&self.0);
        *hasher.finalize().as_bytes()
    }
}

impl SeedCommitment {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
