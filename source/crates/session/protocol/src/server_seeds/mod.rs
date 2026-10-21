use crate::seed_commitment::SeedCommitment;
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

    /// Whether they are seeds of the chain `commitment` commits to: one hash, as each seed is
    /// the hash of the next.
    pub fn commits_to(&self, commitment: &SeedCommitment) -> bool {
        self.seeds[0].commitment() == *commitment
    }

    /// Segment `segment`'s seed; none past the last known.
    pub fn seed(&self, segment: u32) -> Option<&ServerSeed> {
        self.seeds.get(usize::try_from(segment).ok()?)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
