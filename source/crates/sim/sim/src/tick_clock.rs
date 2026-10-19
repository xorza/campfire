use bevy_ecs::system::{Res, SystemParam};
use campfire_common::Tick;

use crate::match_rate::MatchRate;
use crate::sim_tick::SimTick;

/// The sim's clock as a system reads it: the tick under way, and the rate ticks run at.
#[derive(SystemParam, Debug)]
pub struct TickClock<'w> {
    tick: Res<'w, SimTick>,
    rate: Res<'w, MatchRate>,
}

impl TickClock<'_> {
    /// The tick under way, as `SimTick::start` gives it.
    pub fn start(&self) -> Tick {
        self.tick.start()
    }

    /// The end of the tick under way, as `SimTick::end` gives it.
    pub fn end(&self) -> Tick {
        self.tick.end()
    }

    pub fn rate(&self) -> MatchRate {
        *self.rate
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use bevy_ecs::system::RunSystemOnce;
    use bevy_ecs::world::World;

    use super::*;

    #[test]
    fn a_clock_reads_the_tick_under_way_and_the_rate() {
        // Tick 5 runs from 5 to 6, at 30 a second.
        let mut world = World::new();
        world.insert_resource(SimTick::new(Tick::new(5)));
        world.insert_resource(MatchRate::new(NonZeroU32::new(30).unwrap()));
        let read = world.run_system_once(|clock: TickClock<'_>| {
            (clock.start(), clock.end(), clock.rate().hz().get())
        });
        assert_eq!(read.unwrap(), (Tick::new(5), Tick::new(6), 30));
    }
}
