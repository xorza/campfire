use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit's team: its index in the mode's list of teams, of any number. Units of different teams
/// are enemies, so a neutral team for camps is one more index, an enemy of every other.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Team(u8);

impl Team {
    pub const fn new(index: u8) -> Team {
        Team(index)
    }

    pub const fn index(self) -> u8 {
        self.0
    }

    pub const fn is_enemy_of(self, other: Team) -> bool {
        self.0 != other.0
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "units.team";
}
