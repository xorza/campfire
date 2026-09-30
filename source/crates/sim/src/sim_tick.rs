use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};

use crate::sim_state::SimResource;

/// The number of the tick that runs now, and between ticks the number of the next one. A match
/// starts at tick 0. It is state: the tick keys every random draw.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SimTick(u64);

impl SimTick {
    /// A predicting client sets the tick it runs from its network timeline, which a rollback
    /// winds back.
    pub const fn new(tick: u64) -> SimTick {
        SimTick(tick)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The time the tick starts at, in ticks from the match start: the time of its first stages,
    /// and before tick 0 the time of the match start.
    pub const fn start(self) -> u64 {
        self.0
    }

    /// The time the tick ends at: the time of its last stages, one tick after its start.
    pub const fn end(self) -> u64 {
        self.0.checked_add(1).expect("tick numbers exhausted")
    }

    pub(crate) const fn advance(&mut self) {
        self.0 = self.0.checked_add(1).expect("tick numbers exhausted");
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
        let tick = SimTick::new(7);
        assert_eq!((tick.start(), tick.end()), (7, 8));
        assert_eq!(SimTick::new(8).start(), tick.end());
        assert_eq!(SimTick::default().start(), 0);
    }
}
