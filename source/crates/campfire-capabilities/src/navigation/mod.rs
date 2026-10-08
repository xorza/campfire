use std::cmp::Ordering;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Allow, Changed, Has, Or, ROQueryItem, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{
    Capability, EntityIndex, Keyed, Ordered, Position, SimEdge, SimSet, SimTick, StableId,
    StateRegistry, TickRate, Unpredicted,
};

use crate::actions::effect_queues::EffectQueues;
use crate::deliveries::Deliveries;
use crate::deliveries::delivered::{Delivered, Reached};
use crate::deliveries::delivering::Delivering;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::broadphase::Broadphase;
use crate::navigation::collider::Collider;
use crate::navigation::destination::Destination;
use crate::navigation::navigation_column::NavigationColumn;
use crate::navigation::navigation_effect::NavigationEffect;
use crate::navigation::on_path::OnPath;
use crate::navigation::party::{Party, PartyKey};
use crate::navigation::path_walker::PathWalker;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::paths::Paths;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_asks::{AskingUnits, RouteAsks};
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::segment::Segment;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::statics_dirty::StaticsDirty;
use crate::navigation::steering::{Steered, Steering};
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::navigation::walls::Walls;
use crate::stats::move_step::MoveStep;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::body_grid::Placed;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::engine_tag::EngineTag;
use crate::units::forced_move::{DashDelivery, DashTo, ForcedMove, Goal};
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;
use crate::units::unit_tags::UnitTags;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::hit::Hit;
use crate::values::shape::Shape;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod body_index;
pub(crate) mod broadphase;
pub(crate) mod collider;
pub(crate) mod destination;
pub(crate) mod error;
pub(crate) mod group_box;
pub(crate) mod navigation_api;
pub(crate) mod navigation_column;
pub(crate) mod navigation_effect;
pub(crate) mod navigation_rules;
pub(crate) mod on_path;
pub(crate) mod party;
pub(crate) mod path_walker;
pub(crate) mod pathing_grid;
pub(crate) mod paths;
pub(crate) mod progress;
pub(crate) mod regions;
pub(crate) mod route;
pub(crate) mod route_asks;
pub(crate) mod route_planner;
pub(crate) mod segment;
pub(crate) mod static_changes;
pub(crate) mod statics_dirty;
pub(crate) mod steering;
pub(crate) mod terrain;
pub(crate) mod walker;
pub(crate) mod wall;
pub(crate) mod walls;

/// The systems of `navigation`, for the systems of other capabilities to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NavigationSet {
    /// In `SimSet::Inputs`: the static index and the pathing grid take the static bodies as the
    /// tick starts.
    TrackStatics,
}

/// The `navigation` capability: units that walk routes to a destination, the map's waypoint
/// paths, and the forced moves of dashes, knock backs and teleports.
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
    /// routes to their destinations, straight lines until the map gives a pathing grid, then the
    /// forced moves move theirs; in Collide, overlapping living bodies part; after Collide, each
    /// unit that walks stands within the bounds again.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(NavigationColumn::default());
        view.add_source::<RowParts, _>(world, fill_row);
        world.insert_resource(Paths::default());
        world.insert_resource(Bounds::WORLD);
        world.insert_resource(BodyIndex::new(Body::MAX_RADIUS));
        world.insert_resource(StaticChanges::default());
        StaticsDirty::install(world);
        world.insert_resource(ByType::<Walker>::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Navigation, NavigationEffect::queue_listed);
        schedule.add_systems((
            track_static_bodies
                .in_set(SimSet::Inputs)
                .in_set(NavigationSet::TrackStatics),
            (
                forget_dead,
                route_units,
                plan_routes,
                steer,
                move_units,
                force_units,
            )
                .chain()
                .in_set(SimSet::Move),
            (track_static_bodies, collide)
                .chain()
                .in_set(SimSet::Collide),
            keep_in_bounds.in_set(SimEdge::After(SimSet::Collide)),
        ));
        registry.register_component::<Destination>();
        registry.register_component::<PathWalker>();
        registry.register_component::<MoveStep>();
        registry.register_component::<OnPath>();
        registry.register_component::<Route>();
        registry.register_component::<Progress>();
        registry.register_component::<ForcedMove>();
    }

    /// Gives the match the map's pathing grid over `cells`, with the cells `walls` block, for the
    /// kinds of `walkers`, a static index for the widest of them, a planner of routes on the
    /// grid, and the walls for placements to test; the static bodies fill the grid and the index
    /// from the first tick on.
    pub(crate) fn load_pathing(
        world: &mut World,
        cells: Grid,
        walls: &[Wall],
        walkers: Vec<Walker>,
    ) {
        debug_assert!(
            world.contains_resource::<StaticChanges>(),
            "a map's pathing grid is navigation's, which the load checked the mode declares"
        );
        let terrain = Terrain::new(&cells, walls);
        let widest = walkers.iter().map(|walker| walker.radius).max();
        world.insert_resource(BodyIndex::new(widest.unwrap_or(Num::ZERO)));
        world.insert_resource(RoutePlanner::new(&cells));
        world.insert_resource(PathingGrid::new(cells, walkers, &terrain));
        world.insert_resource(Walls::new(walls));
        world.resource_mut::<StaticsDirty>().set();
    }

    /// Makes room for the box of `entity`, which just spawned: each walker of its layer whose
    /// body the box overlaps goes at once to the nearest cell its walker may stand in, by stable
    /// id, as a teleport places a unit but with no disjoint. The static index and the pathing
    /// grid take the box first, so the cell is clear of every static body, and no push of
    /// collision can hold the walker between two. With no pathing grid, the box pushes it out
    /// as collision would.
    pub(crate) fn make_room(world: &mut World, entity: Entity) {
        world
            .run_system_cached(track_static_bodies)
            .expect("the static bodies' tracking runs on any world");
        let unit = world.entity(entity);
        let at = *unit.get::<Position>().expect("a unit has a place");
        let body = *unit.get::<Body>().expect("a box makes room");
        let Shape::Box(boxed) = body.shape() else {
            return;
        };
        let now = world.resource::<SimTick>().start();
        let mut walkers =
            world.query_filtered::<(Entity, &StableId, &Position, &Body), (With<MoveStep>, Without<Dead>)>();
        let mut inside: Vec<Keyed> = walkers
            .iter(world)
            .filter(|&(_, _, &pos, other)| {
                let radius = Walker::walking(Some(other)).radius;
                other.layer() == body.layer() && boxed.nearest(at, pos, radius) == Ordering::Less
            })
            .map(|(entity, &id, ..)| Keyed { id, entity })
            .collect();
        inside.sort_unstable_by_key(|keyed| keyed.id);
        for Keyed { entity: walker, .. } in inside {
            let unit = world.entity(walker);
            let pos = *unit.get::<Position>().expect("a walker has a place");
            let kind = Walker::walking(unit.get::<Body>());
            let grid = world.get_resource::<PathingGrid>();
            let planner = world.get_resource::<RoutePlanner>();
            let place = if let (Some(grid), Some(planner)) = (grid, planner) {
                let walkable = Walkable {
                    clearance: grid.clearance(kind),
                    statics: world.resource::<BodyIndex>(),
                    short: None,
                };
                planner.stand_at(walkable, pos).unwrap_or(pos)
            } else {
                let out = boxed
                    .push_out(at, pos.get(), kind.radius)
                    .unwrap_or(pos.get());
                Position::new(out).expect("a push stays near the bounds")
            };
            NavigationEffect::put(world, walker, place, now);
        }
    }

    /// The center of the cell nearest `point` among those `walker` may stand in, at `point`'s
    /// height, as a teleport finds its cell, with the static bodies as the tick began; `None`
    /// with no pathing grid, or on a grid with no such cell.
    pub(crate) fn open_cell(world: &World, walker: Walker, point: Position) -> Option<Position> {
        let grid = world.get_resource::<PathingGrid>()?;
        let planner = world.get_resource::<RoutePlanner>()?;
        let clearance = grid.serving(walker)?;
        let walkable = Walkable {
            clearance,
            statics: world.resource::<BodyIndex>(),
            short: None,
        };
        let cell = planner.nearest_open(walkable, point, &mut 0)?;
        Some(clearance.grid().center(cell, point.get().y))
    }
}

/// Gives the static index and the pathing grid the static bodies: the living units that cannot
/// walk, those the client only holds among them. It runs as each tick starts, so a structure that
/// died or spawned in the tick before counts from this one, again as Collide starts, so
/// collision parts walkers from the static bodies as they stand then, and as a box spawns, so
/// the walkers it moves out land clear of it. Each change goes to `changes` for the walkers'
/// routes. A run with no static body moved, changed, come or gone since this run of it last ran,
/// or since any run took a body that came or went, ends there; every other run reads them into
/// `statics`, a buffer it keeps.
fn track_static_bodies(
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

/// The parts of a unit navigation reads into its row: the path it is on, whether it walks, and
/// its body, for its layer.
type RowParts = (
    Option<&'static OnPath>,
    Has<MoveStep>,
    Option<&'static Body>,
);

/// Fills a row of the script view with the path the unit walks or stands on, whether it walks,
/// and its layer.
fn fill_row(
    (path, walks, body): ROQueryItem<'_, '_, RowParts>,
    fill: &mut RowFill<'_, NavigationColumn>,
) {
    let path = path.map(|path| path.get());
    fill.column.push(path, walks, Body::layer_of(body));
}

/// Keeps each walker's route on its destination. A walker with a new destination asks for a route
/// there, unless its route reaches its goal and the walker may go straight on from the waypoint
/// before the last to the new one, past every static body and every cell the walls block it from,
/// as a chaser after a target that moved: then only the last waypoint moves. After the static
/// bodies changed, a walker whose way along its route a static body of its layer blocks asks for
/// its route again; after they lost a body, so does one whose route ends short of its goal, and
/// one that arrived short of it walks there again. A walker
/// with no destination forgets its route, unless it arrived short. With no pathing grid, as in a
/// match with no map, no static body blocks a route. Each walker checks its route only when the
/// static bodies changed since the tick before, against the bodies put in, which `changes`
/// holds; as a route the static bodies do not block stays clear until one is put in, the outcome
/// is that of checking every tick. A walker that asks or forgets its route forgets its progress.
/// One on its path asks as one of its spawn group's party, to its goal.
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
            Option<&PathWalker>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    let planned = grid.is_some();
    let opened = planned && changes.removed();
    for (&at, mut destination, mut route, mut progress, body, path) in &mut units {
        let walker = Walker::walking(body);
        let party = |goal| {
            let group = path.filter(|path| !path.left())?.group();
            Some(Party {
                key: PartyKey::Spawn(group),
                goal,
            })
        };
        match destination.get() {
            None if route.arrived_short() => {
                if opened {
                    let goal = route.goal().expect("a route that arrived short has a goal");
                    destination.set(Some(goal));
                    route.ask(goal, now, party(goal));
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
                    route.ask(goal, now, party(goal));
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
                let blocks = |from| {
                    grid.as_ref().is_some_and(|grid| {
                        let walkable = Walkable {
                            clearance: grid.clearance(walker),
                            statics: &statics,
                            short: None,
                        };
                        walkable.blocks(Segment::new(from, goal))
                    })
                };
                if let Some(from) = straight
                    && !blocks(from)
                {
                    route.move_goal(goal);
                } else {
                    route.ask(goal, now, party(goal));
                    progress.restart();
                }
            }
        }
    }
    changes.clear();
}

/// Plans the asked routes as the tick's work begins, as `RouteAsks::plan` says. Every tick reads
/// the asks into `asks`, buffers it keeps.
fn plan_routes(
    grid: Option<Res<'_, PathingGrid>>,
    statics: Res<'_, BodyIndex>,
    mut planner: Option<ResMut<'_, RoutePlanner>>,
    mut units: AskingUnits<'_, '_>,
    mut asks: Local<'_, RouteAsks>,
) {
    asks.plan(
        grid.as_deref(),
        &statics,
        planner.as_deref_mut(),
        &mut units,
    );
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
        (
            With<MoveStep>,
            Without<Dead>,
            Without<ForcedMove>,
            Allow<Unpredicted>,
        ),
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
            Has<ForcedMove>,
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
        .filter(|(.., destination, tags)| !walks(*destination, *tags, false))
        .map(|(&id, &at, body, ..)| IndexedBody::of(id, at, body));
    let walking = bodies
        .iter()
        .filter(|(.., destination, tags)| walks(*destination, *tags, false))
        .map(|(&id, &at, body, ..)| Placed {
            id,
            key: body.layer(),
            at,
            shape: body.shape(),
        });
    let gathering = |tags: Option<&UnitTags>| {
        tags.is_some_and(|tags| tags.tags.contains(EngineTag::Gathering.tag()))
    };
    let gatherers = bodies
        .iter()
        .filter(|&(.., tags)| gathering(tags))
        .map(|(&id, ..)| id);
    steering.read(&statics, still, walking, gatherers);
    let stuck_ticks = rate
        .ticks(Steering::STUCK_MS)
        .expect("a fixed time fits")
        .get();
    let ordered = walkers.iter().map(|(entity, &id, ..)| Keyed { id, entity });
    for &Keyed { entity, .. } in order.sort(ordered) {
        let (_, &id, &at, destination, step, body, mut route, mut progress, tags, forced) =
            walkers.get_mut(entity).expect("a walker in the order");
        if !walks(Some(destination), tags, forced)
            || route.asked().is_some()
            || route.ahead().is_empty()
        {
            continue;
        }
        let steered = Steered {
            id,
            at,
            step: step.get(),
            walker: Walker::walking(Some(body)),
            stuck: u64::from(progress.track(at, step.get())) >= stuck_ticks,
            gathering: gathering(tags),
        };
        if let Some(detour) = steering.steer(planner, &grid, &statics, steered, &route) {
            route.splice(steering.short(), detour.skipped, detour.reached);
            progress.reset();
        }
    }
}

/// Makes each unit that died since the last Move stage forget where it walked to, and ends its
/// forced move. No order reaches a dead unit, so it walks nowhere until it lives again.
fn forget_dead(
    mut forced: Query<'_, '_, Entity, (Added<Dead>, With<ForcedMove>)>,
    mut units: Query<'_, '_, (&mut Destination, &mut Route, &mut Progress), Added<Dead>>,
    mut commands: Commands<'_, '_>,
) {
    for entity in &mut forced {
        commands.entity(entity).remove::<ForcedMove>();
    }
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
/// walks the one it has, if any. One its tags or a forced move stop keeps both.
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
            Has<ForcedMove>,
        ),
        Without<Dead>,
    >,
) {
    for (mut position, mut destination, mut route, mut progress, step, tags, forced) in &mut units {
        if !walks(Some(&destination), tags, forced) {
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
/// order; a pair that only overlaps after this tick's pushes parts in the next. A body a forced
/// move moves passes through the others, and parts from them once the move ends. Only a unit that
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
            Has<ForcedMove>,
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
            .filter(|&(.., movable, _, _, held, forced)| !(forced || held && movable))
            .map(
                |(entity, &id, body, position, movable, destination, tags, ..)| Collider {
                    id,
                    entity,
                    at: position.get(),
                    shape: body.shape(),
                    layer: body.layer(),
                    movable,
                    walking: walks(destination, tags, false),
                    gathering: tags
                        .is_some_and(|tags| tags.tags.contains(EngineTag::Gathering.tag())),
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

/// Whether a unit walks this tick: it has a destination, and neither its tags nor a forced move,
/// when `forced`, stop it. One they stop stands, to the units round it as to itself.
fn walks(destination: Option<&Destination>, tags: Option<&UnitTags>, forced: bool) -> bool {
    destination.is_some_and(|destination| destination.get().is_some())
        && !ForcedMove::blocks(tags, forced, Block::Move)
}

/// Moves each unit a forced move moves, by stable id, once the units walked, so a dash at a unit
/// follows its place of this tick, at the unit's own height. A step whose way a static body of the
/// unit's layer blocks is not taken, nor a knock back's whose way the walls block, but in the cell
/// it starts in; one past the bounds stops on them; either ends the move. A dash crosses walls,
/// and one that ends where its walker may not stand goes to the nearest cell it may, as a teleport
/// does. A unit whose move ended walks its route again from there, and a dash that delivers an
/// action queues that action's end there, its hooks to run in Hit.
fn force_units(
    (tick, bounds, statics, index): (
        Res<'_, SimTick>,
        Res<'_, Bounds>,
        Res<'_, BodyIndex>,
        Res<'_, EntityIndex>,
    ),
    (grid, planner): (Option<Res<'_, PathingGrid>>, Option<Res<'_, RoutePlanner>>),
    mut deliveries: Option<ResMut<'_, Deliveries>>,
    mut units: ParamSet<
        '_,
        '_,
        (
            Query<'_, '_, (&Position, Option<&Body>), (Without<Dead>, Allow<Unpredicted>)>,
            Query<
                '_,
                '_,
                (
                    Entity,
                    &StableId,
                    &mut Position,
                    &mut ForcedMove,
                    Option<&Body>,
                ),
                Without<Dead>,
            >,
        ),
    >,
    mut walkers: Query<'_, '_, (&Destination, &mut Route, &mut Progress)>,
    mut commands: Commands<'_, '_>,
    mut order: Local<'_, Ordered>,
) {
    let now = tick.start();
    let ordered = {
        let moving = units.p1();
        order.sort(moving.iter().map(|(entity, &id, ..)| Keyed { id, entity }))
    };
    for &Keyed { entity, .. } in ordered {
        let (at, mut forced, body) = {
            let moving = units.p1();
            let (_, _, &at, &forced, body) = moving.get(entity).expect("a unit in the order");
            (at, forced, body.copied())
        };
        let walker = Walker::walking(body.as_ref());
        let ground = |place: Vec3| Vec3::new(place.x, at.get().y, place.z);
        let goal = match forced {
            ForcedMove::Dash {
                to: DashTo::Point(point),
                ..
            } => Some(Goal {
                at: ground(point.get()),
                reach: Num::ZERO,
            }),
            ForcedMove::Dash {
                to: DashTo::Unit(target),
                ..
            } => {
                let targets = units.p0();
                let found = index.get(target).and_then(|unit| targets.get(unit).ok());
                // A dash at a box makes for its nearest point, as at a point it must touch.
                found.map(|(&place, body)| match Body::shape_of(body) {
                    Shape::Circle(radius) => Goal {
                        at: ground(place.get()),
                        reach: walker.radius + radius,
                    },
                    Shape::Box(body) => Goal {
                        at: ground(body.nearest_point(place, at).get()),
                        reach: walker.radius,
                    },
                })
            }
            ForcedMove::KnockBack { to, .. } => Some(Goal {
                at: to,
                reach: Num::ZERO,
            }),
        };
        let advanced = forced.advance(at.get(), goal);
        let to = bounds.ground_point([advanced.at.x, advanced.at.z], at);
        let past = to.get() != advanced.at;
        let clearance = grid.as_deref().and_then(|grid| grid.serving(walker));
        let step = Segment::new(at, to);
        let knocked = matches!(forced, ForcedMove::KnockBack { .. });
        let walled = knocked && clearance.is_some_and(|clearance| clearance.walls_block_step(step));
        let blocked = statics.blocks(step, walker) || walled;
        let ends = advanced.ends || past || blocked;
        let stands = if blocked { at } else { to };
        let place = match (clearance, planner.as_deref()) {
            (Some(clearance), Some(planner)) if ends && !knocked => {
                let walkable = Walkable {
                    clearance,
                    statics: &statics,
                    short: None,
                };
                planner.stand_at(walkable, stands).unwrap_or(stands)
            }
            _ => stands,
        };
        let mut moving = units.p1();
        let (_, _, mut position, mut under_way, _) =
            moving.get_mut(entity).expect("a unit in the order");
        position.set_if_neq(place);
        if let ForcedMove::Dash {
            to: dash_to,
            delivers: Some(delivery),
            ..
        } = &mut forced
        {
            delivery.went(at, stands);
            if ends {
                deliveries
                    .as_deref_mut()
                    .expect("a dash that delivers an action runs in a match with abilities")
                    .delivered
                    .push(dash_end(
                        *delivery,
                        *dash_to,
                        Segment::new(at, stands),
                        place,
                    ));
            }
        }
        if ends {
            commands.entity(entity).remove::<ForcedMove>();
            if let Ok((destination, mut route, mut progress)) = walkers.get_mut(entity) {
                route.ask_again(destination, &mut progress, now);
            }
        } else {
            under_way.set_if_neq(forced);
        }
    }
}

/// The end of a dash to `to` that delivers `delivery`, whose last step was `step`, and whose unit
/// then stands at `place`: a hit with no delivery unit, the dash's unit as its target, its place
/// `place`, its distance the way the dash's steps went, and its direction the last step's.
fn dash_end(delivery: DashDelivery, to: DashTo, step: Segment, place: Position) -> Delivered {
    let DashDelivery {
        source,
        action,
        rank,
        start,
        dashed,
    } = delivery;
    let target = match to {
        DashTo::Unit(target) => Some(target),
        DashTo::Point(_) => None,
    };
    Delivered {
        by: Delivering {
            source,
            action,
            rank,
            start,
            launch: None,
        },
        reach: Reached::End,
        hit: Hit {
            delivery: None,
            target,
            pos: place,
            distance: dashed,
            direction: step.start().ground_offset(step.end()).normalized(),
        },
    }
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
