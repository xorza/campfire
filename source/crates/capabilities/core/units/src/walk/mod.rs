use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::StableId;

use crate::arrived_short::ArrivedShort;
use crate::destination::{Destination, WalkGoal};

/// A unit's walk, as a capability sets it: its destination, with its report of the goal it
/// arrived short of, so each write keeps the one rule of a short arrival.
#[derive(QueryData, Debug)]
#[query_data(mutable, derive(Debug))]
pub struct Walk {
    destination: &'static mut Destination,
    arrived: Option<&'static ArrivedShort>,
}

impl Walk {
    /// Sets where the unit of `entity` walks, when it walks, to `target`, by the rule of
    /// `WalkItem::to`, as a capability's own loop walks its unit.
    pub fn go_to(world: &mut World, entity: Entity, target: Option<Vec3>) {
        let arrived = world.get::<ArrivedShort>(entity).copied();
        if let Some(mut destination) = world.get_mut::<Destination>(entity)
            && !short(arrived.as_ref(), target)
        {
            destination.set_if_neq(Destination::of(target.map(WalkGoal::Point)));
        }
    }
}

impl WalkItem<'_, '_> {
    /// Sets where the unit walks, leaving a destination that does not change untouched: a write
    /// marks it changed, and an avatar's destination replicates. A unit that arrived short of
    /// `target` stays where it stands, with no destination, until the static bodies change.
    pub fn to(&mut self, target: Option<Vec3>) {
        self.go(target.map(WalkGoal::Point));
    }

    /// Sets the unit to walk into `unit`, which stands at `at`, by the rule of `to`, so the
    /// walker passes through it on its way (entries-and-blocking.md, A walk may go into a unit).
    pub fn to_unit(&mut self, unit: StableId, at: Vec3) {
        self.go(Some(WalkGoal::Into { unit, at }));
    }

    fn go(&mut self, goal: Option<WalkGoal>) {
        if !short(self.arrived, goal.map(WalkGoal::at)) {
            self.destination.set_if_neq(Destination::of(goal));
        }
    }

    pub fn goal(&self) -> Option<Vec3> {
        self.destination.get()
    }

    /// Whether the unit gives up a walk to `target`: it stands, as it arrived short of `target`.
    pub fn gives_up(&self, target: Vec3) -> bool {
        gives_up(self.destination.get(), self.arrived, target)
    }
}

impl WalkReadOnlyItem<'_, '_> {
    /// Whether the unit gives up a walk to `target`: it stands, as it arrived short of `target`.
    pub fn gives_up(&self, target: Vec3) -> bool {
        gives_up(self.destination.get(), self.arrived, target)
    }
}

fn gives_up(goal: Option<Vec3>, arrived: Option<&ArrivedShort>, target: Vec3) -> bool {
    goal.is_none() && short(arrived, Some(target))
}

fn short(arrived: Option<&ArrivedShort>, target: Option<Vec3>) -> bool {
    target.is_some_and(|target| arrived.is_some_and(|arrived| arrived.of_goal(target)))
}

#[cfg(test)]
mod tests;
