use bevy_ecs::resource::Resource;
use campfire_math::{Rng, RngSource, SegmentSeed};

use crate::sim_tick::SimTick;
use crate::stable_id::StableId;

/// The random sequences of the running tick. The schedule starts each tick's sequences, and
/// opening one takes `&self`, so systems draw in parallel. The seed is not state: it stays secret
/// until its log segment is published.
#[derive(Resource, Debug)]
pub struct SimRng(RngSource);

impl SimRng {
    pub const fn new(seed: SegmentSeed) -> SimRng {
        SimRng(RngSource::new(seed))
    }

    /// The sequence of `stream` for `entity` in the running tick. Debug builds panic when the same
    /// pair opens twice in one tick, since the second would repeat the first one's draws.
    pub fn open(&self, stream: &str, entity: StableId) -> Rng {
        self.0.open(stream, entity.get())
    }

    pub(crate) fn begin_tick(&mut self, tick: SimTick) {
        self.0.begin_tick(tick.get());
    }
}
