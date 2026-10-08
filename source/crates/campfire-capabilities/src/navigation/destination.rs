use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{Position, SimComponent};
use serde::{Deserialize, Serialize};

use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route::Route;

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

    /// Sets where a unit walks, leaving a destination that does not change untouched: a write
    /// marks it changed, and an avatar's destination replicates. A unit whose `route` arrived
    /// short of `target` stays where it stands, with no destination, until the static bodies
    /// change.
    pub(crate) fn walk_to(
        destination: &mut Mut<'_, Destination>,
        route: Option<&Route>,
        target: Option<Position>,
    ) {
        let arrived =
            target.is_some_and(|target| route.is_some_and(|route| route.arrived_short_of(target)));
        if !arrived {
            destination.set_if_neq(Destination::to(target));
        }
    }

    /// Whether a unit with `destination` and `route` gives up a walk to `target`: it cannot
    /// walk, or it stands, as its route arrived short of `target`.
    pub(crate) fn gives_up(
        destination: Option<&Destination>,
        route: Option<&Route>,
        target: Position,
    ) -> bool {
        destination.is_none_or(|destination| {
            destination.get().is_none() && route.is_some_and(|route| route.arrived_short_of(target))
        })
    }

    /// Sets the destination of the unit of `entity`, which walks, to `target`, leaving one that
    /// does not change untouched.
    pub(crate) fn go(world: &mut World, entity: Entity, target: Option<Position>) {
        if let Some(mut destination) = world.get_mut::<Destination>(entity) {
            destination.set_if_neq(Destination::to(target));
        }
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
