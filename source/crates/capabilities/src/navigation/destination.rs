use bevy_ecs::component::Component;
use campfire_sim::{Position, SimComponent};
use serde::{Deserialize, Serialize};

/// Where a unit walks to; none once it arrives.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Destination(Option<Position>);

impl Destination {
    pub const fn get(self) -> Option<Position> {
        self.0
    }

    pub(crate) const fn set(&mut self, target: Option<Position>) {
        self.0 = target;
    }
}

impl SimComponent for Destination {
    const NAME: &'static str = "navigation.destination";
}
