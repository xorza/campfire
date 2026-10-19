use campfire_common::{DomainHasher, HashDomain, Secret};
use serde::{Deserialize, Serialize};

use crate::seed_commitment::SeedCommitment;

/// Starts each link of the seed chain, so no other BLAKE3 use can produce one.
const CHAIN_DOMAIN: HashDomain<SeedCommitment> = HashDomain::new("campfire/seed-chain/v1");

/// A segment's server seed, `s_k` of the session's `SeedChain`. It predicts every hidden random
/// outcome of its segment, so it stays secret until the segment is published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerSeed(Secret<32>);

impl ServerSeed {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(Secret::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Whether this is segment `segment`'s seed of the chain `seed_commitment` commits to: `segment
    /// + 1` links lead from it to the commitment.
    pub fn check(&self, segment: u32, seed_commitment: &SeedCommitment) -> bool {
        let mut seed = self.clone();
        for _ in 0..segment {
            seed = seed.earlier();
        }
        seed.commitment() == *seed_commitment
    }

    /// `C(s_k) = s_(k−1)`, where `C(x) = BLAKE3(domain ‖ x)`.
    pub(crate) fn earlier(&self) -> Self {
        Self(Secret::new(self.link().get()))
    }

    /// `C(s_0)`, when this is the first segment's seed.
    pub(crate) fn commitment(&self) -> SeedCommitment {
        self.link()
    }

    /// `C(s_k)`, which commits to the seed: its commitment, as the first's, or the seed before
    /// it.
    fn link(&self) -> SeedCommitment {
        let mut hasher = DomainHasher::new(CHAIN_DOMAIN);
        hasher.update(self.0.as_bytes());
        hasher.finish()
    }
}
