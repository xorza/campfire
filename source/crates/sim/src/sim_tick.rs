use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};

use crate::sim_state::SimResource;
use crate::tick::{Tick, Ticks};

/// The number of the tick that runs now, and between ticks the number of the next one. A match
/// starts at tick 0. It is state: the tick keys every random draw.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SimTick(Tick);

impl SimTick {
    /// A predicting client sets the tick it runs from its network timeline, which a rollback
    /// winds back.
    pub const fn new(tick: Tick) -> SimTick {
        SimTick(tick)
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
    }
}
