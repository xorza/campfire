use bevy_ecs::resource::Resource;
use campfire_common::{SegmentSeed, Tick};
use campfire_math::{Rng, RngStream};

use crate::sim_tick::SimTick;
use crate::stable_id::StableId;

/// The random sequences of the running tick: its seed, the tick and how many ticks it started. The
/// schedule starts each tick's sequences, and opening one takes `&self`, so the systems that draw
/// read it as a `Res` and do not conflict over it. The seed is not state: it stays secret until its
/// log segment is published.
#[derive(Resource, Debug)]
pub struct SimRng {
    seed: SegmentSeed,
    tick: Tick,
    /// How many ticks it started, a tick run again counted again.
    starts: u64,
    /// The pairs the running tick opened, so a second open of one panics: a set that keeps its
    /// capacity as each tick clears it, so a tick allocates nothing.
    #[cfg(debug_assertions)]
    #[expect(clippy::absolute_paths, reason = "a field of debug builds alone")]
    opened: std::sync::Mutex<bevy_platform::collections::HashSet<(RngStream, u64)>>,
}

/// Opens the sequences of one start of one tick away from the match, for a holder that keeps
/// each sequence it opens for that start, as the debug check of `SimRng` cannot see it. Two
/// openers are equal only for the same start of a tick, so a holder that compares them knows when
/// to drop what it opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RngOpener {
    seed: SegmentSeed,
    tick: Tick,
    /// How many ticks its `SimRng` started before it, a tick run again counted again.
    start: u64,
}

impl SimRng {
    #[cfg_attr(
        debug_assertions,
        expect(clippy::absolute_paths, reason = "a field of debug builds alone")
    )]
    pub const fn new(seed: SegmentSeed) -> Self {
        Self {
            seed,
            tick: Tick::ZERO,
            starts: 0,
            #[cfg(debug_assertions)]
            opened: std::sync::Mutex::new(bevy_platform::collections::HashSet::new()),
        }
    }

    /// The sequence of `stream` for `entity` in the running tick. Debug builds panic when the same
    /// pair opens twice in one tick, since the second would repeat the first one's draws.
    pub fn open(&self, stream: RngStream, entity: StableId) -> Rng {
        let entity = entity.get();
        #[cfg(debug_assertions)]
        {
            let fresh = self
                .opened
                .lock()
                .expect("a draw never panics while it holds the check")
                .insert((stream, entity));
            assert!(
                fresh,
                "RNG stream {stream:?} opened twice for entity {entity} in tick {}",
                self.tick.get()
            );
        }
        Rng::new(&self.seed, stream, entity, self.tick)
    }

    /// What opens the running tick's sequences during a call that holds no world, as a script's
    /// draws do; its holder keeps each sequence it opens for the tick.
    pub fn opener(&self) -> RngOpener {
        RngOpener {
            seed: self.seed.clone(),
            tick: self.tick,
            start: self.starts,
        }
    }

    /// Starts `tick`'s sequences. Running a tick again, as rollback does, starts it again too.
    pub(crate) fn begin_tick(&mut self, tick: SimTick) {
        self.tick = tick.start();
        self.starts += 1;
        #[cfg(debug_assertions)]
        self.opened
            .get_mut()
            .expect("a draw never panics while it holds the check")
            .clear();
    }
}

impl RngOpener {
    /// Whether it opens the sequences of `rng`'s running start of its tick, as `rng`'s opener
    /// does, with no copy of the seed.
    pub fn is_current(&self, rng: &SimRng) -> bool {
        self.seed == rng.seed && self.tick == rng.tick && self.start == rng.starts
    }

    /// The sequence of `stream` for `entity` in its tick, as `SimRng::open` opens it.
    pub fn open(&self, stream: RngStream, entity: StableId) -> Rng {
        Rng::new(&self.seed, stream, entity.get(), self.tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> SegmentSeed {
        SegmentSeed::new(*b"campfire rng test seed, 32 bytes")
    }

    #[test]
    fn a_tick_opens_each_pair_once_and_an_opener_opens_the_same_sequences() {
        let tick = SimTick::new(Tick::new(5));
        let draw = |stream, entity| Rng::new(&seed(), RngStream::new(stream), entity, tick.start());
        let mut rng = SimRng::new(seed());
        rng.begin_tick(tick);
        let a = rng.open(RngStream::new("s"), StableId::new(1)).next_u64();
        assert_eq!(a, draw("s", 1).next_u64());
        assert_ne!(
            rng.open(RngStream::new("s"), StableId::new(2)).next_u64(),
            a
        );
        assert_ne!(
            rng.open(RngStream::new("t"), StableId::new(1)).next_u64(),
            a
        );
        rng.begin_tick(tick);
        assert_eq!(
            rng.open(RngStream::new("s"), StableId::new(1)).next_u64(),
            a
        );
        // An opener opens the same sequences away from the match, and two starts of one tick give
        // two openers, so a holder knows to drop what the first opened.
        let opener = rng.opener();
        assert_eq!(
            opener
                .open(RngStream::new("s"), StableId::new(1))
                .next_u64(),
            a
        );
        assert_eq!(opener, rng.opener());
        rng.begin_tick(tick);
        assert_ne!(opener, rng.opener());
        assert_eq!(
            rng.opener()
                .open(RngStream::new("s"), StableId::new(1))
                .next_u64(),
            a
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "opened twice for entity 1 in tick 5")]
    fn a_tick_panics_on_a_repeated_pair() {
        let mut rng = SimRng::new(seed());
        rng.begin_tick(SimTick::new(Tick::new(5)));
        let _first = rng.open(RngStream::new("s"), StableId::new(1));
        let _second = rng.open(RngStream::new("s"), StableId::new(1));
    }
}
