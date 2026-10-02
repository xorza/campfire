use serde::{Deserialize, Serialize};

use crate::rng::Rng;
use crate::rng::rng_stream::RngStream;

/// The seed of one log segment. It stays secret until the segment is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentSeed([u8; 32]);

impl SegmentSeed {
    pub const fn new(bytes: [u8; 32]) -> SegmentSeed {
        SegmentSeed(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Opens the random sequences of one log segment, one per (stream, entity) in each tick.
#[derive(Debug)]
#[cfg_attr(
    debug_assertions,
    expect(
        clippy::absolute_paths,
        reason = "the debug-only field names its types in full; a cfg'd import may not open the file"
    )
)]
pub struct RngSource {
    seed: SegmentSeed,
    tick: u64,
    #[cfg(debug_assertions)]
    opened: std::sync::Mutex<std::collections::BTreeSet<(RngStream, u64)>>,
}

impl RngSource {
    #[cfg_attr(
        debug_assertions,
        expect(
            clippy::absolute_paths,
            reason = "the debug-only field names its types in full; a cfg'd import may not open the file"
        )
    )]
    pub const fn new(seed: SegmentSeed) -> RngSource {
        RngSource {
            seed,
            tick: 0,
            #[cfg(debug_assertions)]
            opened: std::sync::Mutex::new(std::collections::BTreeSet::new()),
        }
    }

    /// Starts `tick`. Running a tick again, as rollback does, starts it again too.
    pub fn begin_tick(&mut self, tick: u64) {
        self.tick = tick;
        #[cfg(debug_assertions)]
        self.opened
            .get_mut()
            .expect("RNG registry poisoned")
            .clear();
    }

    /// The sequence of `stream` for `entity` in the current tick. Debug builds panic when the
    /// same pair opens twice in one tick, since the second would repeat the first one's draws.
    pub fn open(&self, stream: RngStream, entity: u64) -> Rng {
        #[cfg(debug_assertions)]
        {
            let fresh = self
                .opened
                .lock()
                .expect("RNG registry poisoned")
                .insert((stream, entity));
            assert!(
                fresh,
                "RNG stream {stream:?} opened twice for entity {entity} in tick {}",
                self.tick
            );
        }
        Rng::new(&self.seed, stream, entity, self.tick)
    }
}
