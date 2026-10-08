use std::mem;

use bevy_ecs::lifecycle::{Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut};
use bevy_ecs::world::World;
use campfire_sim::{Position, StableId, Unpredicted};

use crate::stats::move_step::MoveStep;
use crate::units::body::Body;
use crate::units::dead::Dead;

/// Whether a unit may have become a static body, or stopped being one, since the static index
/// last took the static bodies: a living unit that cannot walk. A static body that moves or
/// changes its body shows in the change ticks; one that comes or goes changes no component of
/// the set, so observers mark it here. It only lets the tracking skip, so a mark too many costs
/// one full pass, never a wrong index. Derived, never state.
#[derive(Resource, Debug)]
pub(crate) struct StaticsDirty(bool);

/// Whether each unit with a body, a place and a stable id walks, and whether it is dead.
type BodyParts<'w, 's> = Query<
    'w,
    's,
    (Has<MoveStep>, Has<Dead>),
    (
        With<StableId>,
        With<Position>,
        With<Body>,
        Allow<Unpredicted>,
    ),
>;

impl StaticsDirty {
    /// Gives `world` the mark, set, so the first tracking takes every static body, and the
    /// observers that set it.
    pub(crate) fn install(world: &mut World) {
        world.insert_resource(StaticsDirty(true));
        world.add_observer(StaticsDirty::part_removed);
        world.add_observer(StaticsDirty::state_inserted);
        world.add_observer(StaticsDirty::state_removed);
    }

    /// Sets the mark, as the static bodies need taking again.
    pub(crate) const fn set(&mut self) {
        self.0 = true;
    }

    /// Clears the mark, and tells whether it was set.
    pub(crate) const fn take(&mut self) -> bool {
        mem::replace(&mut self.0, false)
    }

    /// A static body loses a part it needs, or is despawned.
    fn part_removed(
        removed: On<'_, '_, Remove, (Position, Body, StableId)>,
        parts: BodyParts<'_, '_>,
        mut dirty: ResMut<'_, StaticsDirty>,
    ) {
        if parts
            .get(removed.entity)
            .is_ok_and(|(walks, dead)| !walks && !dead)
        {
            dirty.set();
        }
    }

    /// A unit dies or walks: it was static if, as it is now, it lacks the other of the two.
    fn state_inserted(
        inserted: On<'_, '_, Insert, (MoveStep, Dead)>,
        parts: BodyParts<'_, '_>,
        mut dirty: ResMut<'_, StaticsDirty>,
    ) {
        if parts
            .get(inserted.entity)
            .is_ok_and(|(walks, dead)| !(walks && dead))
        {
            dirty.set();
        }
    }

    /// A unit lives again or stops walking: it becomes static if it lacks the other of the two.
    /// The removed one is still on it as the observer runs.
    fn state_removed(
        removed: On<'_, '_, Remove, (MoveStep, Dead)>,
        parts: BodyParts<'_, '_>,
        mut dirty: ResMut<'_, StaticsDirty>,
    ) {
        if parts
            .get(removed.entity)
            .is_ok_and(|(walks, dead)| !(walks && dead))
        {
            dirty.set();
        }
    }
}
