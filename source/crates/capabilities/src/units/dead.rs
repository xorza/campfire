use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// Marks a unit that died and stays for the mode to respawn. A dead unit takes no orders, does not
/// move and is no target.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dead;

impl SimComponent for Dead {
    const NAME: &'static str = "combat.dead";
}
