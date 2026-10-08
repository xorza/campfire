use bevy_ecs::query::{Added, Allow, Changed, Or, With, Without};
use bevy_ecs::system::{Local, Query, ResMut};
use campfire_sim::{Position, StableId, Unpredicted};

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::statics_dirty::StaticsDirty;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::move_step::MoveStep;

/// The static bodies: the living units that cannot walk, those the client only holds among them.
type StaticBodies<'w, 's> = Query<
    'w,
    's,
    (&'static StableId, &'static Position, &'static Body),
    (Without<MoveStep>, Without<Dead>, Allow<Unpredicted>),
>;

/// The static bodies whose place or body changed, or that are new.
type ChangedStatics<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<StableId>,
        With<Position>,
        With<Body>,
        Or<(Changed<Position>, Changed<Body>, Added<StableId>)>,
        Without<MoveStep>,
        Without<Dead>,
        Allow<Unpredicted>,
    ),
>;

/// The static bodies the static index and the pathing grid hold, and their changes.
#[derive(Debug)]
pub(super) struct StaticTracking;

impl StaticTracking {
    /// Gives the static index and the pathing grid the static bodies: the living units that cannot
    /// walk, those the client only holds among them. It runs as each tick starts, so a structure
    /// that died or spawned in the tick before counts from this one, again as Collide starts, so
    /// collision parts walkers from the static bodies as they stand then, and as a box spawns, so
    /// the walkers it moves out land clear of it. Each change goes to `changes` for the walkers'
    /// routes. A run with no static body moved, changed, come or gone since this run of it last
    /// ran, or since any run took a body that came or went, ends there; every other run reads them
    /// into `statics`, a buffer it keeps.
    pub(super) fn track_static_bodies(
        mut index: ResMut<'_, BodyIndex>,
        (mut changes, mut dirty): (ResMut<'_, StaticChanges>, ResMut<'_, StaticsDirty>),
        grid: Option<ResMut<'_, PathingGrid>>,
        bodies: StaticBodies<'_, '_>,
        moved: ChangedStatics<'_, '_>,
        mut statics: Local<'_, Vec<IndexedBody>>,
    ) {
        if !dirty.take() && moved.is_empty() {
            return;
        }
        statics.clear();
        statics.extend(
            bodies
                .iter()
                .map(|(&id, &at, body)| IndexedBody::of(id, at, body)),
        );
        statics.sort_unstable_by_key(|body| body.id);
        if !index.update(&statics) {
            return;
        }
        changes.note(&index);
        if let Some(mut grid) = grid {
            grid.update(&index);
        }
    }
}
