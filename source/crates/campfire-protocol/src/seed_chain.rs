use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::server_seed::{SeedCommitment, ServerSeed};
use crate::server_seeds::ServerSeeds;

/// Every segment's server seed, fixed when the server opens the session, as a one-way hash chain:
/// segment `k`'s seed is the root hashed `N − 1 − k` times, so each seed is the hash of the next.
/// Revealing a seed reveals every earlier one and no later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedChain {
    /// `s_(N−1)`, the last segment's seed. It is random, and the server keeps it secret.
    root: ServerSeed,
    /// `N`, the most segments the session may have.
    len: NonZeroU32,
}

impl SeedChain {
    pub const fn new(root: [u8; 32], len: NonZeroU32) -> SeedChain {
        SeedChain {
            root: ServerSeed::new(root),
            len,
        }
    }

    /// `s_k`, the root hashed `N − 1 − k` times.
    pub fn seed(&self, segment: u32) -> ServerSeed {
        assert!(
            segment < self.len.get(),
            "segment {segment} is past the chain's {} segments",
            self.len
        );
        let mut seed = self.root;
        for _ in segment + 1..self.len.get() {
            seed = seed.earlier();
        }
        seed
    }

    /// `C(s_0)`, what the session's terms commit to.
    /// Every segment's seed, as the server that holds the chain knows them.
    pub const fn seeds(&self) -> ServerSeeds {
        ServerSeeds::new(self.len.get() - 1, self.root)
    }

    pub fn commitment(&self) -> SeedCommitment {
        self.seed(0).commitment()
    }
}

#[cfg(test)]
mod tests {
    use blake3::Hasher;

    use super::*;

    const CHAIN: SeedChain = SeedChain::new([5; 32], NonZeroU32::new(2).unwrap());

    #[test]
    fn each_seed_hashes_to_the_one_before_and_the_first_to_the_commitment() {
        let digest = |bytes: &[u8]| {
            let mut hasher = Hasher::new();
            hasher.update(b"campfire/seed-chain/v1").update(bytes);
            *hasher.finalize().as_bytes()
        };
        // s_1 is the root, s_0 its hash, and the commitment the hash of s_0.
        let (s0, s1) = (CHAIN.seed(0), CHAIN.seed(1));
        assert_eq!(s1.as_bytes(), &[5; 32]);
        assert_eq!(s0.as_bytes(), &digest(&[5; 32]));
        let commitment = CHAIN.commitment();
        assert_eq!(commitment.as_bytes(), &digest(s0.as_bytes()));
        // A seed checks for its own segment only: 1 hash leads from s_0 to the commitment, 2
        // from s_1.
        assert!(s0.check(0, &commitment));
        assert!(s1.check(1, &commitment));
        assert!(!s1.check(0, &commitment));
        assert!(!s0.check(1, &commitment));
        // A chain of one segment has its root as that segment's seed.
        let single = SeedChain::new([5; 32], NonZeroU32::MIN);
        assert_eq!(single.seed(0), s1);
        assert!(s1.check(0, &single.commitment()));
    }

    #[test]
    #[should_panic(expected = "segment 2 is past the chain's 2 segments")]
    fn a_seed_past_the_chain_is_a_bug() {
        CHAIN.seed(2);
    }
}
