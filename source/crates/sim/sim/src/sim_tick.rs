use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use serde::{Deserialize, Serialize};

use crate::sim_state::SimResource;

/// The number of the tick that runs now, and between ticks the number of the next one. A match
/// starts at tick 0. It is state: the tick keys every random draw.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SimTick(Tick);

impl SimTick {
    /// The most ticks a server runs in one frame after a stall, the time past them dropped and
    /// logged, so a frame costs at most this many ticks' work, as a fixed time step bounds its
    /// catch-up (08-sessions.md, A burst after a stall runs 9 ticks at most); and a state that
    /// holds this many ticks from a tick on is in the world at the send of the frame that ran it.
    pub const FRAME_MOST: Ticks = Ticks::new(9);

    /// A predicting client sets the tick it runs from its network timeline, which a rollback
    /// winds back.
    pub const fn new(tick: Tick) -> Self {
        Self(tick)
    }

    /// The time the tick starts at, in ticks from the match start: the time of its first stages,
    /// and before tick 0 the time of the match start.
    pub const fn start(self) -> Tick {
        self.0
    }

    /// The time the tick ends at: the time of its last stages, one tick after its start.
    pub const fn end(self) -> Tick {
        self.0.after(Ticks::ONE)
    }

    pub(crate) const fn advance(&mut self) {
        self.0 = self.end();
    }
}

impl SimResource for SimTick {
    const NAME: &'static str = "sim.tick";

    // Its decode keeps it within the limit.
    fn check(&self, _: &World) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_ends_where_the_next_starts() {
        let tick = SimTick::new(Tick::new(7));
        assert_eq!((tick.start(), tick.end()), (Tick::new(7), Tick::new(8)));
        assert_eq!(SimTick::new(Tick::new(8)).start(), tick.end());
        assert_eq!(SimTick::default().start(), Tick::ZERO);
        // The last tick a match makes plays on past the limit.
        let mut last = SimTick::new(Tick::LIMIT);
        last.advance();
        assert_eq!(last.start(), Tick::new(Tick::LIMIT.get() + 1));
    }
}
