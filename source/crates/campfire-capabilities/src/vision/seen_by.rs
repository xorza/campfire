use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::units::team_set::TeamSet;

/// The teams that see a unit, as the Vision stage of the last tick found them: those of its own
/// vision group, and of each group with a living unit that sees the grid cell it stands in.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeenBy(TeamSet);

impl SeenBy {
    pub(crate) const fn new(teams: TeamSet) -> SeenBy {
        SeenBy(teams)
    }

    pub const fn get(self) -> TeamSet {
        self.0
    }
}

impl SimComponent for SeenBy {
    const NAME: &'static str = "vision.seen_by";

    // A set of teams, which a reader only asks about the teams it has.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}
