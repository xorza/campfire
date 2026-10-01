use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit its AI reset: it walks to its spawn place and takes no order until it arrives, when its
/// pools are full again.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Resetting;

impl SimComponent for Resetting {
    const NAME: &'static str = "orders.resetting";
}
