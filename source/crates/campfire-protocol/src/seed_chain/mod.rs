use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::seed_commitment::SeedCommitment;
use crate::server_seed::ServerSeed;
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

    /// Every segment's seed, as the server that holds the chain knows them.
    pub const fn seeds(&self) -> ServerSeeds {
        ServerSeeds::new(self.len.get() - 1, self.root)
    }

    /// `C(s_0)`, what the session's terms commit to.
    pub fn commitment(&self) -> SeedCommitment {
        self.seed(0).commitment()
    }
}

#[cfg(test)]
mod tests;
