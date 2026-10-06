use crate::server_seed::ServerSeed;

/// The server seeds a holder knows: every segment's up to `last`, from `last`'s, as each seed is
/// the hash of the next. The server knows its whole chain, from the root; a verifier knows the
/// seeds a published log reveals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSeeds {
    last: u32,
    seed: ServerSeed,
}

impl ServerSeeds {
    /// The seeds up to segment `last`, whose seed is `seed`.
    pub const fn new(last: u32, seed: ServerSeed) -> ServerSeeds {
        ServerSeeds { last, seed }
    }

    /// The last segment whose seed is known.
    pub const fn last(&self) -> u32 {
        self.last
    }

    /// Segment `segment`'s seed; none past the last known.
    pub fn seed(&self, segment: u32) -> Option<ServerSeed> {
        if segment > self.last {
            return None;
        }
        let mut seed = self.seed;
        for _ in segment..self.last {
            seed = seed.earlier();
        }
        Some(seed)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::seed_chain::SeedChain;

    #[test]
    fn the_seeds_from_a_segment_are_the_chains_up_to_it() {
        let chain = SeedChain::new([5; 32], NonZeroU32::new(3).unwrap());
        let all = chain.seeds();
        assert_eq!(all.last(), 2);
        assert_eq!(
            [0, 1, 2].map(|segment| all.seed(segment)),
            [0, 1, 2].map(|segment| Some(chain.seed(segment)))
        );
        assert_eq!(all.seed(3), None);
        let revealed = ServerSeeds::new(1, chain.seed(1));
        assert_eq!(revealed.seed(0), Some(chain.seed(0)));
        assert_eq!(revealed.seed(1), Some(chain.seed(1)));
        assert_eq!(revealed.seed(2), None);
    }
}
