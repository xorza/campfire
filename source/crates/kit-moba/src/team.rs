use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit's side. The first team's creeps walk each lane forward, the second team's backward.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Team {
    First,
    Second,
    /// Camps and objectives: an enemy of both sides.
    Neutral,
}

impl Team {
    pub const fn is_enemy_of(self, other: Team) -> bool {
        !matches!(
            (self, other),
            (Team::First, Team::First)
                | (Team::Second, Team::Second)
                | (Team::Neutral, Team::Neutral)
        )
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "moba.team";
}
