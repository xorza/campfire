use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// The tick a unit that thinks is due in again. A unit whose call found the think pool spent
/// keeps it, so it stays due, and goes first in the next tick, before the units due later.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NextThink(u64);

impl NextThink {
    pub const fn new(tick: u64) -> NextThink {
        NextThink(tick)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl SimComponent for NextThink {
    const NAME: &'static str = "control.next_think";
}
