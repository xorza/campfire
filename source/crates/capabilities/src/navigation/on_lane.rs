use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// The lane a unit that does not walk belongs to, such as a structure: `unit.lane` reads it, as
/// it reads a walker's.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OnLane(u32);

impl OnLane {
    pub const fn new(lane: u32) -> OnLane {
        OnLane(lane)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl SimComponent for OnLane {
    const NAME: &'static str = "navigation.on_lane";
}
