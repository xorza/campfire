use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};

use crate::sim_state::SimResource;

/// The number of the tick that runs now, and between ticks the number of the next one. A match
/// starts at tick 0. It is state: the tick keys every random draw.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SimTick(u64);

impl SimTick {
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn advance(&mut self) {
        self.0 = self.0.checked_add(1).expect("tick numbers exhausted");
    }
}

impl SimResource for SimTick {
    const NAME: &'static str = "sim.tick";
}
