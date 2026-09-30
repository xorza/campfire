use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::units::lane::Lane;

/// The lane a unit belongs to: the one a structure guards, or the one a `LaneWalker` walks.
/// `unit.lane` reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OnLane(Lane);

impl OnLane {
    pub const fn new(lane: Lane) -> OnLane {
        OnLane(lane)
    }

    pub const fn get(self) -> Lane {
        self.0
    }
}

impl SimComponent for OnLane {
    const NAME: &'static str = "navigation.on_lane";
}
