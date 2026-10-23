use bevy_ecs::resource::Resource;
use campfire_common::{Tick, Ticks};
use campfire_net_core::NetTick;
use serde::{Deserialize, Serialize};

/// Maps the stack's ticks to sim ticks: sim tick `first` is the stack's tick `start`, the tick
/// the match started in, or ran on in after a restore, so the ticks a crash lost take no time in
/// the sim.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchClock {
    start: NetTick,
    first: Tick,
}

impl MatchClock {
    /// The clock of a match that runs sim tick `first` in `start`.
    pub const fn new(start: NetTick, first: Tick) -> Self {
        Self { start, first }
    }

    /// The same clock from sim tick `first` on, at or after its own first: what a client that
    /// joins the match there runs by, as it holds no sim tick before.
    #[must_use]
    pub fn since(self, first: Tick) -> Self {
        Self::new(self.net_tick(first), first)
    }

    /// The stack's tick that is sim tick `first`.
    pub const fn start(self) -> NetTick {
        self.start
    }

    pub const fn first(self) -> Tick {
        self.first
    }

    /// The stack's tick of the sim tick `tick`, at or after `first`.
    pub fn net_tick(self, tick: Tick) -> NetTick {
        let since = tick
            .since(self.first)
            .expect("a tick at or after the clock's first");
        let since = u32::try_from(since.get()).expect("a match's ticks fit the stack's");
        NetTick(
            self.start
                .0
                .checked_add(since)
                .expect("a match's ticks fit the stack's"),
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
        // A match that starts, its sim tick 0 at the stack's tick 100.
        let fresh = MatchClock::new(NetTick(100), Tick::new(0));
        assert_eq!(fresh.sim_tick(NetTick(99)), None);
        assert_eq!(fresh.sim_tick(NetTick(100)), Some(Tick::new(0)));
        assert_eq!(fresh.sim_tick(NetTick(103)), Some(Tick::new(3)));
        // Restored at sim tick 5000 in a server whose timeline runs from 0.
        let resumed = MatchClock::new(NetTick(7), Tick::new(5000));
        assert_eq!(resumed.sim_tick(NetTick(6)), None);
        assert_eq!(resumed.sim_tick(NetTick(7)), Some(Tick::new(5000)));
        assert_eq!(resumed.sim_tick(NetTick(9)), Some(Tick::new(5002)));
        assert_eq!(resumed.net_tick(Tick::new(5002)), NetTick(9));
        assert_eq!(fresh.net_tick(Tick::new(0)), NetTick(100));
        // From sim tick 4 on, the fresh clock starts at the stack's tick 104, and holds no tick
        // before.
        let joined = fresh.since(Tick::new(4));
        assert_eq!(
            (joined.start(), joined.first()),
            (NetTick(104), Tick::new(4))
        );
        assert_eq!(joined.sim_tick(NetTick(103)), None);
        assert_eq!(joined.sim_tick(NetTick(105)), fresh.sim_tick(NetTick(105)));
    }
}
