use campfire_common::SegmentSeed;

use crate::rng::Rng;
use crate::rng::rng_stream::RngStream;

/// Opens the sequences of one start of one tick away from their source, for a holder that keeps
/// each sequence it opens for that start, as the source's debug check cannot see it. Two openers
/// are equal only for the same start of a tick, so a holder that compares them knows when to drop
/// what it opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RngOpener {
    pub(crate) seed: SegmentSeed,
    pub(crate) tick: u64,
    /// How many ticks its source started before it, a tick run again counted again.
    pub(crate) start: u64,
}

impl RngOpener {
    /// The sequence of `stream` for `entity` in its tick.
    pub fn open(&self, stream: RngStream, entity: u64) -> Rng {
        Rng::new(&self.seed, stream, entity, self.tick)
    }
}
