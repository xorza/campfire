use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// Marks a hero that died and waits to respawn. A dead unit takes no orders and is no target;
/// other units despawn when they die.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dead;

impl SimComponent for Dead {
    const NAME: &'static str = "moba.dead";
}
