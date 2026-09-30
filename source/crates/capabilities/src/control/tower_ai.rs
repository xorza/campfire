use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// Marks a tower: it keeps its target while that target lives and stays in range, and otherwise
/// attacks the nearest enemy in range, the lower stable id on a tie. It stands in for the tower
/// AI script until scripts run.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TowerAi;

impl SimComponent for TowerAi {
    const NAME: &'static str = "control.tower_ai";
}
