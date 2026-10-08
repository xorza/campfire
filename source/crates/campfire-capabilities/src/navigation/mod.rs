use std::cmp::Ordering;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{
    Capability, Keyed, Position, SimEdge, SimSet, SimTick, StableId, StateRegistry,
};

use crate::actions::effect_queues::EffectQueues;
use crate::geometry::grid::Grid;
use crate::geometry::shape::Shape;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::forced_moves::ForcedMoves;
use crate::navigation::navigation_column::{NavigationColumn, RowParts};
use crate::navigation::navigation_effect::NavigationEffect;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::paths::Paths;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::routing::Routing;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::static_tracking::StaticTracking;
use crate::navigation::statics_dirty::StaticsDirty;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::walking::Walking;
use crate::navigation::wall::Wall;
use crate::navigation::walls::Walls;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::move_step::MoveStep;
use crate::units::view::View;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod body_index;
pub(crate) mod broadphase;
pub(crate) mod collider;
pub(crate) mod destination;
pub(crate) mod error;
pub(crate) mod forced_moves;
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
pub(crate) mod routing;
pub(crate) mod segment;
pub(crate) mod static_changes;
pub(crate) mod static_tracking;
pub(crate) mod statics_dirty;
pub(crate) mod steering;
pub(crate) mod terrain;
pub(crate) mod walker;
pub(crate) mod walking;
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
        view.add_source::<RowParts, _>(world, NavigationColumn::fill_row);
        world.insert_resource(Paths::default());
        world.insert_resource(BodyIndex::new(Shape::MAX_BOUND));
        world.insert_resource(StaticChanges::default());
        StaticsDirty::install(world);
        world.insert_resource(ByType::<Walker>::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Navigation, NavigationEffect::queue_listed);
        schedule.add_systems((
            StaticTracking::track_static_bodies
                .in_set(SimSet::Inputs)
                .in_set(NavigationSet::TrackStatics),
            (
                Walking::forget_dead,
                Routing::route_units,
                Routing::plan_routes,
                Walking::steer,
                Walking::move_units,
                ForcedMoves::force_units,
            )
                .chain()
                .in_set(SimSet::Move),
            (StaticTracking::track_static_bodies, Walking::collide)
                .chain()
                .in_set(SimSet::Collide),
            Walking::keep_in_bounds.in_set(SimEdge::After(SimSet::Collide)),
        ));
        registry.register_component::<Destination>();
        registry.register_component::<PathWalker>();
        registry.register_component::<OnPath>();
        registry.register_component::<Route>();
        registry.register_component::<Progress>();
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
            .run_system_cached(StaticTracking::track_static_bodies)
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
                let walkable = Walkable::of(grid.clearance(kind), world.resource::<BodyIndex>());
                planner.stand_at(walkable, pos).unwrap_or(pos)
            } else {
                let out = boxed
                    .push_out(at, pos.get(), kind.radius)
                    .expect("the box overlaps the walker, so it pushes the walker out");
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
        let walkable = Walkable::of(clearance, world.resource::<BodyIndex>());
        let cell = planner.nearest_open(walkable, point, &mut 0)?;
        Some(clearance.grid().center(cell, point.get().y))
    }
}

#[cfg(test)]
mod tests;
