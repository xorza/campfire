use bevy_ecs::resource::Resource;
use lightyear::prelude::Tick;

/// Maps Lightyear's ticks to sim ticks: sim tick 0 is the Lightyear tick the match started in.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchClock {
    start: Tick,
}

impl MatchClock {
    pub const fn new(start: Tick) -> MatchClock {
        MatchClock { start }
    }

    /// The sim tick of `tick`; `None` before the match starts.
    pub fn sim_tick(self, tick: Tick) -> Option<u64> {
        tick.0.checked_sub(self.start.0).map(u64::from)
    }
}
