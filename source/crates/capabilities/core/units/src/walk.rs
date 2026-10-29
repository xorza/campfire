use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::world::World;
use campfire_math::Vec3;

use crate::arrived_short::ArrivedShort;
use crate::destination::Destination;

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
            destination.set_if_neq(Destination::of(target));
        }
    }
}

impl WalkItem<'_, '_> {
    /// Sets where the unit walks, leaving a destination that does not change untouched: a write
    /// marks it changed, and an avatar's destination replicates. A unit that arrived short of
    /// `target` stays where it stands, with no destination, until the static bodies change.
    pub fn to(&mut self, target: Option<Vec3>) {
        if !short(self.arrived, target) {
            self.destination.set_if_neq(Destination::of(target));
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
mod tests {
    use bevy_ecs::world::World;
    use campfire_math::Vec3;

    use crate::arrived_short::ArrivedShort;
    use crate::destination::Destination;
    use crate::walk::{Walk, WalkReadOnly};

    #[test]
    fn a_walk_to_a_goal_it_arrived_short_of_keeps_the_unit_standing() {
        let short = Vec3::from((4, 0, 0));
        let other = Vec3::from((0, 0, 4));
        let mut world = World::new();
        let unit = world
            .spawn((Destination::default(), ArrivedShort::of(Some(short))))
            .id();
        let goal = |world: &World, entity| world.get::<Destination>(entity).unwrap().get();
        let gives_up = |world: &mut World, entity, to| {
            let mut walks = world.query::<WalkReadOnly>();
            walks.get(world, entity).unwrap().gives_up(to)
        };
        // Short of `short`, the unit stands and gives up a walk there, not one to `other`.
        assert!(gives_up(&mut world, unit, short));
        assert!(!gives_up(&mut world, unit, other));
        Walk::go_to(&mut world, unit, Some(short));
        assert_eq!(goal(&world, unit), None);
        // The same rule holds through the query, and a walk anywhere else goes, as a stop does.
        let mut walks = world.query::<Walk>();
        walks.get_mut(&mut world, unit).unwrap().to(Some(short));
        assert_eq!(goal(&world, unit), None);
        assert!(walks.get_mut(&mut world, unit).unwrap().gives_up(short));
        walks.get_mut(&mut world, unit).unwrap().to(Some(other));
        assert_eq!(goal(&world, unit), Some(other));
        assert!(!gives_up(&mut world, unit, short));
        walks.get_mut(&mut world, unit).unwrap().to(None);
        assert_eq!(goal(&world, unit), None);
    }
}
