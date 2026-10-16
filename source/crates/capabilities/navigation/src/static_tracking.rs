use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, ResMut, SystemId};
use campfire_declare::OneShot;
use campfire_units::Tracking;

use crate::body_index::{BodyIndex, IndexedBody};
use crate::pathing_grid::PathingGrid;
use crate::static_bodies::StaticBodies;
use crate::static_changes::StaticChanges;

/// The static bodies the static index and the pathing grid hold, and their changes.
#[derive(Debug)]
pub struct StaticTracking;

/// The tracking of the static bodies as a system of its own, which a spawn runs out of the
/// schedule, as `Navigation::make_room` does.
#[derive(Resource, Debug, Clone, Copy)]
pub struct StaticTrackingRun(pub(super) SystemId);

impl OneShot for StaticTrackingRun {
    type In = ();

    fn new(id: SystemId) -> Self {
        Self(id)
    }
}

impl StaticTracking {
    /// Gives the static index and the pathing grid the static bodies: the living units that cannot
    /// walk, those the client only holds among them. It runs as each tick starts, so a structure
    /// that died or spawned in the tick before counts from this one, again as Collide starts, so
    /// collision parts walkers from the static bodies as they stand then, and as a box spawns, so
    /// the walkers it moves out land clear of it. Each change goes to `changes` for the walkers'
    /// routes. A run whose tracker finds the static bodies as the last run took them ends there;
    /// every other run reads them into `taken`, a buffer it keeps.
    pub(super) fn track_static_bodies(
        mut index: ResMut<'_, BodyIndex>,
        mut changes: ResMut<'_, StaticChanges>,
        grid: Option<ResMut<'_, PathingGrid>>,
        mut statics: Tracking<'_, '_, StaticBodies>,
        mut taken: Local<'_, Vec<IndexedBody>>,
    ) {
        if !statics.stale() {
            return;
        }
        taken.clear();
        taken.extend(statics.units().map(|unit| {
            let body = unit.body.expect("a static body has its body");
            IndexedBody::of(unit.id, unit.pos, body)
        }));
        statics.took(taken.len());
        taken.sort_unstable_by_key(|body| body.id);
        if !index.update(&taken) {
            return;
        }
        changes.note(&index);
        if let Some(mut grid) = grid {
            grid.update(&index);
        }
    }
}
