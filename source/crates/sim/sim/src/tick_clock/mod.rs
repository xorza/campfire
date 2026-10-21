use bevy_ecs::system::{Res, SystemParam};
use campfire_common::Tick;

use crate::match_rate::MatchRate;
use crate::sim_tick::SimTick;

/// The sim's clock as a system reads it: the tick under way, and the rate ticks run at.
#[derive(SystemParam, Debug)]
pub struct TickClock<'w> {
    tick: Res<'w, SimTick>,
    tick_rate: Res<'w, MatchRate>,
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
        *self.tick_rate
    }
}

#[cfg(test)]
mod tests;
