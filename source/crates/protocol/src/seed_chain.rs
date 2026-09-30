use std::num::NonZeroU32;

use crate::server_seed::{SeedCommitment, ServerSeed};

/// Every segment's server seed, fixed when the server opens the session, as a one-way hash chain:
/// segment `k`'s seed is the root hashed `N − 1 − k` times, so each seed is the hash of the next.
/// Revealing a seed reveals every earlier one and no later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub fn commitment(&self) -> SeedCommitment {
        self.seed(0).commitment()
    }
}
