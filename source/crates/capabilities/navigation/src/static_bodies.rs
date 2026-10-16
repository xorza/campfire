use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, Has, With, Without};
use bevy_ecs::system::{Query, ResMut};
use campfire_sim::{Position, StableId, Unpredicted};
use campfire_units::{Body, Dead, MoveStep, TrackedUnits, UnitTracker};

/// The static bodies: the living units with a body that cannot walk, those the client only holds
/// among them, which the static index and the pathing grid hold. A static body that moves or
/// changes its body shows in the change ticks; one that comes or goes changes no part the tracker
/// holds, so the observers here mark it.
#[derive(Debug)]
pub struct StaticBodies;

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

impl TrackedUnits for StaticBodies {
    type Filter = (With<Body>, Without<MoveStep>, Without<Dead>);
}

impl StaticBodies {
    /// A static body loses a part it needs, or is despawned.
    pub(super) fn part_removed(
        removed: On<'_, '_, Remove, (Position, Body, StableId)>,
        parts: BodyParts<'_, '_>,
        mut tracker: ResMut<'_, UnitTracker<Self>>,
    ) {
        if parts
            .get(removed.entity)
            .is_ok_and(|(walks, dead)| !walks && !dead)
        {
            tracker.mark();
        }
    }

    /// A unit dies or walks: it was static if, as it is now, it lacks the other of the two.
    pub(super) fn state_inserted(
        inserted: On<'_, '_, Insert, (MoveStep, Dead)>,
        parts: BodyParts<'_, '_>,
        tracker: ResMut<'_, UnitTracker<Self>>,
    ) {
        Self::state_changed(inserted.entity, &parts, tracker);
    }

    /// A unit lives again or stops walking: it becomes static if it lacks the other of the two.
    /// The removed one is still on it as the observer runs.
    pub(super) fn state_removed(
        removed: On<'_, '_, Remove, (MoveStep, Dead)>,
        parts: BodyParts<'_, '_>,
        tracker: ResMut<'_, UnitTracker<Self>>,
    ) {
        Self::state_changed(removed.entity, &parts, tracker);
    }

    /// `entity` gained or lost walking or being dead: it was or becomes static unless it has
    /// both, and so stays a walker or dead either way.
    fn state_changed(
        entity: Entity,
        parts: &BodyParts<'_, '_>,
        mut tracker: ResMut<'_, UnitTracker<Self>>,
    ) {
        if parts
            .get(entity)
            .is_ok_and(|(walks, dead)| !(walks && dead))
        {
            tracker.mark();
        }
    }
}
