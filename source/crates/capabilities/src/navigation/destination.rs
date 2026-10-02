use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{Position, SimComponent};
use serde::{Deserialize, Serialize};

use crate::navigation::pathing_grid::PathingGrid;

/// Where a unit walks to; none once it arrives.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Destination(Option<Position>);

impl Destination {
    /// A destination of `target`, none for no target.
    pub(crate) const fn to(target: Option<Position>) -> Destination {
        Destination(target)
    }

    pub const fn get(self) -> Option<Position> {
        self.0
    }

    pub(crate) const fn set(&mut self, target: Option<Position>) {
        self.0 = target;
    }
}

impl SimComponent for Destination {
    const NAME: &'static str = "navigation.destination";

    // Its decode keeps the point within the bound; a unit that walks to it steers over the cells
    // of its kind of walker, which must be one the mode has.
    fn check(&self, world: &World, entity: Entity) -> bool {
        PathingGrid::serves(world, entity)
    }
}
