use bevy_ecs::resource::Resource;
use campfire_sim::Tick;
use lightyear::prelude::Tick as NetTick;

/// Maps Lightyear's ticks to sim ticks: sim tick 0 is the Lightyear tick the match started in.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchClock {
    start: NetTick,
}

impl MatchClock {
    pub const fn new(start: NetTick) -> MatchClock {
        MatchClock { start }
    }

    /// The sim tick of `tick`; `None` before the match starts.
    pub fn sim_tick(self, tick: NetTick) -> Option<Tick> {
        let since = tick.0.checked_sub(self.start.0)?;
        Some(Tick::new(u64::from(since)))
    }
}
