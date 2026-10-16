use crate::server_seed::ServerSeed;

/// The server seeds a holder knows: every segment's up to the last, each derived once from the
/// last's as the holder takes them, as each seed is the hash of the next. The server knows its
/// whole chain, from the root; a verifier knows the seeds a published log reveals. A segment's
/// seed is read as the segment starts, in the server's tick, so it costs no hash there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSeeds {
    /// Each segment's seed, from the first's.
    seeds: Vec<ServerSeed>,
}

impl ServerSeeds {
    /// The seeds up to segment `last`, whose seed is `server_seed`: `last` hashes.
    pub fn new(last: u32, server_seed: ServerSeed) -> Self {
        let count = usize::try_from(last).expect("segments fit usize") + 1;
        let mut seeds = vec![server_seed; count];
        for at in (0..count - 1).rev() {
            seeds[at] = seeds[at + 1].earlier();
        }
        Self { seeds }
    }

    /// The last segment whose seed is known.
    pub fn last(&self) -> u32 {
        u32::try_from(self.seeds.len() - 1).expect("a segment fits u32")
    }

    /// Segment `segment`'s seed; none past the last known.
    pub fn seed(&self, segment: u32) -> Option<ServerSeed> {
        self.seeds.get(usize::try_from(segment).ok()?).copied()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

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
        // A chain of the lobby's length, each seed the chain's own, which hashes from the root.
        let long = SeedChain::new([7; 32], NonZeroU32::new(1024).unwrap());
        let seeds = long.seeds();
        assert_eq!(seeds.last(), 1023);
        assert!((0..1024).all(|segment| seeds.seed(segment) == Some(long.seed(segment))));
        assert_eq!(seeds.seed(1024), None);
        assert_eq!(
            ServerSeeds::new(0, long.seed(0)).seed(0),
            Some(long.seed(0))
        );
        let revealed = ServerSeeds::new(1, chain.seed(1));
        assert_eq!(revealed.seed(0), Some(chain.seed(0)));
        assert_eq!(revealed.seed(1), Some(chain.seed(1)));
        assert_eq!(revealed.seed(2), None);
    }
}
