use bevy_ecs::resource::Resource;
use campfire_common::{Tick, Ticks};
use lightyear::prelude::Tick as NetTick;

/// Maps Lightyear's ticks to sim ticks: sim tick `first` is the Lightyear tick `start`, the tick
/// the match started in, or ran on in after a restore, so the ticks a crash lost take no time in
/// the sim.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchClock {
    start: NetTick,
    first: Tick,
}

impl MatchClock {
    /// The clock of a match that starts, sim tick 0, in `start`.
    pub const fn new(start: NetTick) -> MatchClock {
        MatchClock::resumed(start, Tick::new(0))
    }

    /// The clock of a match that runs sim tick `first` in `start`.
    pub const fn resumed(start: NetTick, first: Tick) -> MatchClock {
        MatchClock { start, first }
    }

    /// The Lightyear tick of the sim tick `tick`, at or after `first`.
    pub fn net_tick(self, tick: Tick) -> NetTick {
        let since = tick
            .since(self.first)
            .expect("a tick at or after the clock's first");
        let since = u32::try_from(since.get()).expect("a match's ticks fit Lightyear's");
        NetTick(
            self.start
                .0
                .checked_add(since)
                .expect("a match's ticks fit Lightyear's"),
        )
    }

    /// The sim tick of `tick`; `None` before the match starts, or runs on.
    pub fn sim_tick(self, tick: NetTick) -> Option<Tick> {
        let since = tick.0.checked_sub(self.start.0)?;
        Some(self.first.after(Ticks::new(u64::from(since))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resumed_clock_runs_its_first_sim_tick_in_its_start() {
        let fresh = MatchClock::new(NetTick(100));
        assert_eq!(fresh.sim_tick(NetTick(99)), None);
        assert_eq!(fresh.sim_tick(NetTick(100)), Some(Tick::new(0)));
        assert_eq!(fresh.sim_tick(NetTick(103)), Some(Tick::new(3)));
        // Restored at sim tick 5000 in a server whose timeline runs from 0.
        let resumed = MatchClock::resumed(NetTick(7), Tick::new(5000));
        assert_eq!(resumed.sim_tick(NetTick(6)), None);
        assert_eq!(resumed.sim_tick(NetTick(7)), Some(Tick::new(5000)));
        assert_eq!(resumed.sim_tick(NetTick(9)), Some(Tick::new(5002)));
        assert_eq!(resumed.net_tick(Tick::new(5002)), NetTick(9));
        assert_eq!(fresh.net_tick(Tick::new(0)), NetTick(100));
    }
}
