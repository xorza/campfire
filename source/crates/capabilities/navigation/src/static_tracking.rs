use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, Query, Res, ResMut, SystemId};
use campfire_common::Tick;
use campfire_declare::OneShot;
use campfire_sim::{DueIndex, SimTick, StableId};
use campfire_units::{Body, Dead, Destination, Tracking};

use crate::body_index::{BodyIndex, IndexedBody};
use crate::path_walker::PathWalker;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;
use crate::route::{Route, RouteAsk, RouteDues, RouteMut};
use crate::static_bodies::StaticBodies;
use crate::walker::Walker;

/// The static bodies the static index and the pathing grid hold, and the walkers' routes they
/// change.
#[derive(Debug)]
pub struct StaticTracking;

/// The tracking of the static bodies as a system of its own, which a spawn runs out of the
/// schedule, as `Navigation::make_room` does.
#[derive(Resource, Debug, Clone, Copy)]
pub struct StaticTrackingRun(pub(super) SystemId);

impl OneShot for StaticTrackingRun {
    type In = ();
    type Out = ();

    fn new(id: SystemId) -> Self {
        Self(id)
    }
}

/// The parts of a living walker whose route a change of the static bodies may ask again.
type RoutedWalkers<'w, 's> = Query<
    'w,
    's,
    (
        (Entity, &'static StableId),
        &'static mut Destination,
        RouteMut,
        &'static mut Progress,
        Option<&'static Body>,
        Option<&'static PathWalker>,
    ),
    Without<Dead>,
>;

impl StaticTracking {
    /// Gives the static index and the pathing grid the static bodies: the living units that cannot
    /// walk, those the client only holds among them. It runs as each tick starts, so a new or a
    /// restored match fills them; again as Collide starts, so collision parts walkers from the
    /// static bodies as they stand then; as a box spawns, so the walkers it moves out land clear
    /// of it; and once the tick's last stage is done, so the index holds the bodies as they stand
    /// at each boundary. A run whose tracker finds the static bodies as the last run took them
    /// ends there; every other run reads them into `taken`, a buffer it keeps, and applies their
    /// change to the walkers' routes, as `follow_statics` says. A new index's first fill changes
    /// no route: its bodies stood there before it, as after a restore or a late join.
    pub(super) fn track_static_bodies(
        tick: Res<'_, SimTick>,
        mut index: ResMut<'_, BodyIndex>,
        grid: Option<ResMut<'_, PathingGrid>>,
        mut statics: Tracking<'_, '_, StaticBodies>,
        mut walkers: RoutedWalkers<'_, '_>,
        (mut taken, mut asks): (Local<'_, Vec<IndexedBody>>, ResMut<'_, DueIndex<Route>>),
    ) {
        if !statics.is_stale() {
            return;
        }
        taken.clear();
        taken.extend(statics.units().map(|unit| {
            let body = unit.body.expect("a static body has its body");
            IndexedBody::of(unit.id, unit.position, body)
        }));
        statics.took(taken.len());
        taken.sort_unstable_by_key(|body| body.id);
        let filled = index.is_filled();
        if !index.update(&taken) {
            return;
        }
        let Some(mut grid) = grid else {
            return;
        };
        grid.update(&index);
        if filled {
            Self::follow_statics(tick.start(), &index, &mut walkers, &mut asks);
        }
    }

    /// Follows the last update of `statics`, on a pathing grid, in `now`: a walker whose route a
    /// static body it put in blocks, one of its layer that comes closer to a leg of the route
    /// than the walker's radius, asks for its route again; after it took a body away, so does
    /// one whose route ends short of its goal, and one that arrived short of it walks there
    /// again. A route reads its own legs, from where the leg to its next waypoint began, so the
    /// outcome holds wherever its walker stands. One with an ask waiting keeps it. A walker that
    /// asks forgets its progress; one on its path asks as one of its spawn group's party.
    fn follow_statics(
        now: Tick,
        statics: &BodyIndex,
        walkers: &mut RoutedWalkers<'_, '_>,
        asks: &mut DueIndex<Route>,
    ) {
        let opened = !statics.removed().is_empty();
        let added = !statics.added().is_empty();
        for ((entity, &id), mut destination, mut route, mut progress, body, path) in
            walkers.iter_mut()
        {
            let dues = &mut RouteDues::new(asks, id, entity);
            let ask = |goal| RouteAsk {
                goal,
                tick: now,
                party: path.and_then(|path| path.party(goal)),
            };
            match destination.get() {
                None if opened && route.read().is_arrived_short() => {
                    let goal = route.goal().expect("a route that arrived short has a goal");
                    destination.set_if_neq(Destination::to(Some(goal)));
                    route.ask(ask(goal), &mut progress, dues);
                }
                Some(goal) if route.goal() == Some(goal) && route.asked().is_none() => {
                    let walker = Walker::walking(body);
                    let blocked = added
                        && route
                            .read()
                            .legs()
                            .any(|leg| statics.added_blocks(leg, walker));
                    if blocked || (opened && !route.is_reached()) {
                        route.ask(ask(goal), &mut progress, dues);
                    }
                }
                _ => {}
            }
        }
    }
}
