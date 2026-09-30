use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// The player whose orders a unit follows, by slot.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Controller(u32);

impl Controller {
    pub const fn new(slot: u32) -> Controller {
        Controller(slot)
    }

    pub const fn slot(self) -> u32 {
        self.0
    }
}

impl SimComponent for Controller {
    const NAME: &'static str = "moba.controller";
}
