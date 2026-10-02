use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Allow, Has, ROQueryItem, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{
    Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry, TickRate, Unpredicted,
};

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::broadphase::Broadphase;
use crate::navigation::collider::Collider;
use crate::navigation::destination::Destination;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::paths::Paths;
use crate::navigation::paths_column::PathsColumn;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Waiting, Walkable};
use crate::navigation::segment::Segment;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::steering::{Steered, Steering};
use crate::navigation::walker::Walker;
use crate::stats::move_step::MoveStep;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::body_grid::Placed;
use crate::units::dead::Dead;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;
use crate::units::unit_tags::UnitTags;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod body_index;
pub(crate) mod broadphase;
pub(crate) mod collider;
pub(crate) mod destination;
pub(crate) mod error;
pub(crate) mod navigation_api;
pub(crate) mod navigation_rules;
pub(crate) mod on_path;
pub(crate) mod path_walker;
pub(crate) mod pathing_grid;
pub(crate) mod paths;
pub(crate) mod paths_column;
pub(crate) mod progress;
pub(crate) mod regions;
pub(crate) mod route;
pub(crate) mod route_planner;
pub(crate) mod segment;
pub(crate) mod static_changes;
pub(crate) mod steering;
pub(crate) mod walker;

/// The `navigation` capability: units that walk routes to a destination, and the map's waypoint
/// paths.
#[derive(Debug)]
pub struct Navigation;

impl Navigation {
    /// The components of a new unit that walks `step` a tick, with nowhere to go yet.
    pub fn walker(step: MoveStep) -> impl Bundle {
        (
            step,
            Destination::default(),
            Route::default(),
            Progress::default(),
        )
    }

    /// Adds navigation to a match, with no paths, the world for bounds, and a static index for
    /// walkers as wide as a body may be, until the mode sets its map's: in Move, units walk their
    /// routes to their destinations, straight lines until the map gives a pathing grid; in Collide,
    /// overlapping living bodies part; after Collide, each unit that walks stands within the bounds
    /// again.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(PathsColumn::default());
        view.add_source::<RowParts>(world, fill_row);
        world.insert_resource(Paths::default());
        world.insert_resource(Bounds::WORLD);
        world.insert_resource(BodyIndex::new(Body::MAX_RADIUS));
        world.insert_resource(StaticChanges::default());
        schedule.add_systems((
            track_static_bodies.in_set(SimSet::Inputs),
            (forget_dead, route_units, plan_routes, steer, move_units)
                .chain()
                .in_set(SimSet::Move),
            (track_static_bodies, collide)
                .chain()
                .in_set(SimSet::Collide),
            keep_in_bounds.after(SimSet::Collide).before(SimSet::Hit),
        ));
        registry.register_component::<Destination>();
        registry.register_component::<PathWalker>();
        registry.register_component::<MoveStep>();
        registry.register_component::<OnPath>();
        registry.register_component::<Route>();
        registry.register_component::<Progress>();
    }

    /// Gives the match the map's pathing grid over `cells`, for the kinds of `walkers`, a static
    /// index for the widest of them, and a planner of routes on the grid; the static bodies fill
    /// the grid and the index from the first tick on.
    pub fn load_pathing(world: &mut World, cells: Grid, walkers: Vec<Walker>) {
        let widest = walkers.iter().map(|walker| walker.radius).max();
        world.insert_resource(BodyIndex::new(widest.unwrap_or(Num::ZERO)));
        world.insert_resource(RoutePlanner::new(&cells));
        world.insert_resource(PathingGrid::new(cells, walkers));
    }
}

/// Gives the static index and the pathing grid the static bodies: the living units that cannot
/// walk, those the client only holds among them. It runs as each tick starts, so a structure that
/// died or spawned in the tick before counts from this one, and again as Collide starts, so
/// collision parts walkers from the static bodies as they stand then. Each change goes to
/// `changes` for the walkers' routes. Every run reads them into `statics`, a buffer it keeps.
fn track_static_bodies(
    mut index: ResMut<'_, BodyIndex>,
    mut changes: ResMut<'_, StaticChanges>,
    grid: Option<ResMut<'_, PathingGrid>>,
    bodies: Query<
        '_,
        '_,
        (&StableId, &Position, &Body),
        (Without<MoveStep>, Without<Dead>, Allow<Unpredicted>),
    >,
    mut statics: Local<'_, Vec<IndexedBody>>,
) {
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

/// The part of a unit navigation reads into its row: the path it is on.
type RowParts = Option<&'static OnPath>;

/// Fills a row of the script view with the path the unit walks or stands on.
fn fill_row(path: ROQueryItem<'_, '_, RowParts>, fill: &mut RowFill<'_>) {
    let path = path.map(|path| path.get());
    fill.column::<PathsColumn>().push(path);
}

/// Keeps each walker's route on its destination. A walker with a new destination asks for a route
/// there, unless its route reaches its goal and the walker may go straight on from the waypoint
/// before the last to the new one, as a chaser after a target that moved: then only the last
/// waypoint moves. After the static bodies changed, a walker whose way along its route a static
/// body of its layer blocks asks for its route again; after they lost a body, so does one whose
/// route ends short of its goal, and one that arrived short of it walks there again. A walker
/// with no destination forgets its route, unless it arrived short. With no pathing grid, as in a
/// match with no map, no static body blocks a route. Each walker checks its route only when the
/// static bodies changed since the tick before, against the bodies put in, which `changes`
/// holds; as a route the static bodies do not block stays clear until one is put in, the outcome
/// is that of checking every tick. A walker that asks or forgets its route forgets its progress.
fn route_units(
    tick: Res<'_, SimTick>,
    (statics, mut changes): (Res<'_, BodyIndex>, ResMut<'_, StaticChanges>),
    grid: Option<Res<'_, PathingGrid>>,
    mut units: Query<
        '_,
        '_,
        (
            &Position,
            &mut Destination,
            &mut Route,
            &mut Progress,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    let planned = grid.is_some();
    let opened = planned && changes.removed();
    for (&at, mut destination, mut route, mut progress, body) in &mut units {
        let walker = Walker::of(body);
        match destination.get() {
            None if route.arrived_short() => {
                if opened {
                    let goal = route.goal().expect("a route that arrived short has a goal");
                    destination.set(Some(goal));
                    route.ask(goal, now);
                    progress.restart();
                }
            }
            None => {
                if route.goal().is_some() {
                    route.clear();
                    progress.restart();
                }
            }
            Some(goal) if route.goal() == Some(goal) => {
                let blocked = planned && changes.blocks_route(at, route.ahead(), walker);
                let unreached = opened && !route.reached();
                if route.asked().is_none() && (blocked || unreached) {
                    route.ask(goal, now);
                    progress.restart();
                }
            }
            Some(goal) => {
                let from = match route.ahead() {
                    [.., before, _] => Some(*before),
                    [_] => Some(at),
                    [] => None,
                };
                let straight = from.filter(|_| route.reached() && route.asked().is_none());
                if let Some(from) = straight
                    && !(planned && statics.blocks(Segment::new(from, goal), walker))
                {
                    route.move_goal(goal);
                } else {
                    route.ask(goal, now);
                    progress.restart();
                }
            }
        }
    }
    changes.clear();
}

/// Plans the asked routes, by the tick they were asked in, then by stable id, as the tick's work
/// begins, until the planner did all the work a tick may: the route that meets that limit
/// finishes, and the rest wait for the next tick, so a tick plans at most one search over the
/// whole grid past the limit. With no pathing grid each route is the straight line to its goal.
/// Every tick reads the asks into `waiting`, and each route into `waypoints`, buffers it keeps.
fn plan_routes(
    grid: Option<Res<'_, PathingGrid>>,
    statics: Res<'_, BodyIndex>,
    mut planner: Option<ResMut<'_, RoutePlanner>>,
    mut units: Query<
        '_,
        '_,
        (Entity, &StableId, &Position, &mut Route, Option<&Body>),
        Without<Dead>,
    >,
    mut waiting: Local<'_, Vec<Waiting>>,
    mut waypoints: Local<'_, Vec<Position>>,
) {
    waiting.clear();
    waiting.extend(units.iter().filter_map(|(entity, &id, _, route, _)| {
        Some(Waiting {
            tick: route.asked()?,
            id,
            entity,
        })
    }));
    waiting.sort_unstable();
    if let Some(planner) = planner.as_deref_mut() {
        planner.begin_tick();
    }
    for next in &*waiting {
        let (_, _, &at, mut route, body) =
            units.get_mut(next.entity).expect("a unit read this tick");
        let goal = route.goal().expect("a unit that asked a route has a goal");
        let Some(grid) = &grid else {
            route.answer(&[goal], true);
            continue;
        };
        let planner = planner
            .as_deref_mut()
            .expect("a pathing grid comes with its planner");
        if planner.spent() {
            break;
        }
        let walkable = Walkable {
            clearance: grid.clearance(Walker::of(body)),
            statics: &statics,
            short: None,
        };
        let outcome = planner.plan(walkable, at, goal, &mut waypoints);
        route.answer(&waypoints, outcome.reached);
    }
}

/// Steers each walker round the units in their way, as `Steering::steer` says, in stable-id
/// order, with the work the planner has left this tick; once it has none, the rest keep their
/// routes, and steer again in a later tick. A walker is stuck once it has moved less than half a
/// step a tick for `Steering::STUCK_MS`, and its progress starts again when it takes a short
/// route. A predicting client counts the units it holds that stand: one that stands has not
/// moved since the server sent it. With no pathing grid no walker steers, and one that does not
/// walk this tick, or waits for its route, does not either.
fn steer(
    rate: Res<'_, TickRate>,
    grid: Option<Res<'_, PathingGrid>>,
    statics: Res<'_, BodyIndex>,
    mut planner: Option<ResMut<'_, RoutePlanner>>,
    bodies: Query<
        '_,
        '_,
        (
            &StableId,
            &Position,
            &Body,
            Option<&Destination>,
            Option<&UnitTags>,
        ),
        (With<MoveStep>, Without<Dead>, Allow<Unpredicted>),
    >,
    mut walkers: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Position,
            &Destination,
            &MoveStep,
            &Body,
            &mut Route,
            &mut Progress,
            Option<&UnitTags>,
        ),
        Without<Dead>,
    >,
    (mut steering, mut order): (Local<'_, Steering>, Local<'_, Ordered>),
) {
    let Some(grid) = grid else {
        return;
    };
    let planner = planner
        .as_deref_mut()
        .expect("a pathing grid comes with its planner");
    let still = bodies
        .iter()
        .filter(|(.., destination, tags)| !walks(*destination, *tags))
        .map(|(&id, &at, body, ..)| IndexedBody::of(id, at, body));
    let walking = bodies
        .iter()
        .filter(|(.., destination, tags)| walks(*destination, *tags))
        .map(|(&id, &at, body, ..)| Placed {
            id,
            key: body.layer(),
            at,
            radius: body.radius(),
        });
    steering.read(&statics, still, walking);
    let stuck_ticks = rate
        .ticks(Steering::STUCK_MS)
        .expect("a fixed time fits")
        .get();
    let ordered = walkers.iter().map(|(entity, &id, ..)| Keyed { id, entity });
    for &Keyed { entity, .. } in order.sort(ordered) {
        let (_, &id, &at, destination, step, body, mut route, mut progress, tags) =
            walkers.get_mut(entity).expect("a walker in the order");
        if !walks(Some(destination), tags) || route.asked().is_some() || route.ahead().is_empty() {
            continue;
        }
        let steered = Steered {
            id,
            at,
            step: step.get(),
            walker: Walker::of(Some(body)),
            stuck: u64::from(progress.track(at, step.get())) >= stuck_ticks,
        };
        if let Some(detour) = steering.steer(planner, &grid, &statics, steered, &route) {
            route.splice(steering.short(), detour.skipped, detour.reached);
            progress.reset();
        }
    }
}

/// Makes each unit that died since the last Move stage forget where it walked to. No order
/// reaches a dead unit, so it walks nowhere until it lives again.
fn forget_dead(
    mut units: Query<'_, '_, (&mut Destination, &mut Route, &mut Progress), Added<Dead>>,
) {
    for (mut destination, mut route, mut progress) in &mut units {
        if destination.get().is_some() {
            destination.set(None);
        }
        if route.goal().is_some() {
            route.clear();
            progress.restart();
        }
    }
}

/// Walks each living unit along its route, a step a tick: past each waypoint it reaches, on to
/// the next with the rest of its step. Past the last it has arrived, and drops its destination,
/// and its route when the route reached the goal: one that ends short stays, arrived short, so an
/// order that sends the unit there again plans nothing. A unit whose route waits for the planner
/// walks the one it has, if any. One its tags stop keeps both.
fn move_units(
    mut units: Query<
        '_,
        '_,
        (
            &mut Position,
            &mut Destination,
            &mut Route,
            &mut Progress,
            &MoveStep,
            Option<&UnitTags>,
        ),
        Without<Dead>,
    >,
) {
    for (mut position, mut destination, mut route, mut progress, step, tags) in &mut units {
        if !walks(Some(&destination), tags) {
            continue;
        }
        let mut at = position.get();
        let mut left = step.get();
        loop {
            let Some(&waypoint) = route.ahead().first() else {
                if route.asked().is_none() {
                    destination.set(None);
                    if route.reached() {
                        route.clear();
                        progress.restart();
                    }
                }
                break;
            };
            let target = waypoint.get();
            if !at.within(target, left) {
                at = at.step_toward(target, left);
                break;
            }
            left -= at.distance(target);
            at = target;
            route.advance();
        }
        position.set_if_neq(
            Position::new(at).expect("a step ends between two points within the bound"),
        );
    }
}

/// Parts the living bodies of one layer that overlap as the stage starts, pair by pair in stable-id
/// order; a pair that only overlaps after this tick's pushes parts in the next. Only a unit that
/// can walk is pushed, and one walking to a destination yields to one that stands. The static
/// bodies' contacts come from `statics`, which holds them as the stage starts. A predicting client
/// also parts its own units from the units it holds as the server sent them that cannot walk, such
/// as towers, which never move. Every other held unit is where the server last had it, behind the
/// client's ticks, and may have started or stopped walking since, so the server alone parts the
/// client's units from it. Every tick reads the bodies into `colliders`, and finds their contacts
/// with `broadphase`, buffers it keeps.
fn collide(
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Body,
            &mut Position,
            Has<MoveStep>,
            Option<&Destination>,
            Option<&UnitTags>,
            Has<Unpredicted>,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
    statics: Res<'_, BodyIndex>,
    mut colliders: Local<'_, Vec<Collider>>,
    mut broadphase: Local<'_, Broadphase>,
) {
    colliders.clear();
    colliders.extend(
        units
            .iter()
            .filter(|&(.., movable, _, _, held)| !(held && movable))
            .map(
                |(entity, &id, body, position, movable, destination, tags, _)| Collider {
                    id,
                    entity,
                    at: position.get(),
                    radius: body.radius(),
                    layer: body.layer(),
                    movable,
                    walking: walks(destination, tags),
                },
            ),
    );
    colliders.sort_unstable_by_key(|collider| collider.id);
    debug_assert_eq!(
        colliders
            .iter()
            .filter(|collider| !collider.movable)
            .count(),
        statics.len(),
        "the static index holds the static bodies as the stage starts"
    );
    let contacts = broadphase.contacts(&colliders, &statics);
    Collider::resolve(&mut colliders, contacts);
    for collider in &*colliders {
        let (_, _, _, mut position, ..) = units
            .get_mut(collider.entity)
            .expect("a body read this tick");
        let parted = Position::new(collider.at).expect("a push stays near the bounds");
        position.set_if_neq(parted);
    }
}

/// Whether a unit walks this tick: it has a destination, and its tags let it move. One they
/// stop stands, to the units round it as to itself.
fn walks(destination: Option<&Destination>, tags: Option<&UnitTags>) -> bool {
    destination.is_some_and(|destination| destination.get().is_some())
        && !UnitTags::effects_of(tags).blocks(Block::Move)
}

/// Clamps each unit that walks into the bounds; a unit already within them does not change.
fn keep_in_bounds(
    bounds: Res<'_, Bounds>,
    mut units: Query<'_, '_, &mut Position, With<MoveStep>>,
) {
    for mut position in &mut units {
        position.set_if_neq(bounds.clamp(*position));
    }
}

#[cfg(test)]
mod tests;
