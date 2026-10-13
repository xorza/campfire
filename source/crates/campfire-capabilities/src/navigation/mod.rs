use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{Capability, Position, SimEdge, SimPlace, SimSet};

use crate::actions::effect_queues::EffectQueues;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::set_order::SetOrder;
use crate::geometry::grid::Grid;
use crate::geometry::shape::Shape;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::forced_moves::ForcedMoves;
use crate::navigation::navigation_api::NavigationApi;
use crate::navigation::navigation_book::NavigationBook;
use crate::navigation::navigation_column::{NavigationColumn, RowReads};
use crate::navigation::navigation_effect::NavigationEffect;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::progress::Progress;
use crate::navigation::room_making::{RoomMaking, RoomMakingRun};
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::routing::Routing;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::static_tracking::{StaticTracking, StaticTrackingRun};
use crate::navigation::statics_dirty::StaticsDirty;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::walking::Walking;
use crate::units::body_unit::BodyUnit;
use crate::units::by_type::ByType;
use crate::units::move_step::MoveStep;

#[cfg(feature = "internals")]
pub(crate) mod bench;
pub(crate) mod body_index;
pub(crate) mod broadphase;
pub(crate) mod collider;
pub(crate) mod destination;
pub(crate) mod error;
pub(crate) mod forced_moves;
pub(crate) mod group_box;
pub(crate) mod mode_navigation_data;
pub(crate) mod navigation_api;
pub(crate) mod navigation_book;
pub(crate) mod navigation_column;
pub(crate) mod navigation_effect;
pub(crate) mod on_path;
pub(crate) mod party;
pub(crate) mod path_walker;
pub(crate) mod pathing_grid;
pub(crate) mod paths;
pub(crate) mod progress;
pub(crate) mod regions;
pub(crate) mod room_making;
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

campfire_common::enum_all! {
    /// Navigation's steps, for the capabilities built on it to order theirs against.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum NavigationSet {
        /// The static index and the pathing grid take the static bodies as the tick starts.
        TrackStatics,
        /// Units route and move.
        Move,
        /// The static bodies are taken again, and walkers collide.
        Collide,
        /// The units out of the map's bounds come back in.
        Bounds,
    }
    /// Every step.
    pub(crate) const ALL;
}

impl ModuleSet for NavigationSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Navigation);
    const ALL: &'static [NavigationSet] = &NavigationSet::ALL;

    fn place(self) -> SimPlace {
        match self {
            NavigationSet::TrackStatics => SimPlace::Stage(SimSet::Inputs),
            NavigationSet::Move => SimPlace::Stage(SimSet::Move),
            NavigationSet::Collide => SimPlace::Stage(SimSet::Collide),
            NavigationSet::Bounds => SimPlace::Edge(SimEdge::After(SimSet::Collide)),
        }
    }

    fn order(self, _: &mut SetOrder) {
        match self {
            NavigationSet::TrackStatics
            | NavigationSet::Move
            | NavigationSet::Collide
            | NavigationSet::Bounds => {}
        }
    }
}

/// The `navigation` capability: units that walk routes to a destination, the map's waypoint
/// paths, and the forced moves of dashes, knock backs and teleports.
#[derive(Debug)]
pub struct Navigation;

impl Module for Navigation {
    const CAPABILITY: Option<Capability> = Some(Capability::Navigation);
    const NEEDS: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<NavigationSet>();
        d.component::<Destination>();
        d.component::<PathWalker>();
        d.component::<OnPath>();
        d.component::<Route>();
        d.component::<Progress>();
        d.scratch::<StaticChanges>();
        d.cache::<StaticsDirty>();
        d.cache::<BodyIndex>();
        d.cache::<PathingGrid>();
        d.cache::<RoutePlanner>();
        d.cache::<StaticTrackingRun>();
        d.cache::<RoomMakingRun>();
        d.callback::<EffectQueues>(Capability::Navigation, NavigationEffect::queue_listed);
        d.book::<ByType<Walker>>();
        d.book::<NavigationBook>();
        d.column::<NavigationColumn, RowReads>(NavigationColumn::fill_row);
        d.api(NavigationApi::register);
        // A unit that walks, which its move step tells, takes a destination, a route and a progress.
        d.part::<BodyUnit, Destination>(|at| {
            at.of_type::<MoveStep>().map(|_| Destination::default())
        });
        d.part::<BodyUnit, Route>(|at| at.of_type::<MoveStep>().map(|_| Route::default()));
        d.part::<BodyUnit, Progress>(|at| at.of_type::<MoveStep>().map(|_| Progress::default()));
    }

    /// Adds navigation to a match, with no paths, the world for bounds, and a static index for
    /// walkers as wide as a body may be, until the mode sets its map's: in Move, units walk their
    /// routes to their destinations, straight lines until the map gives a pathing grid, then the
    /// forced moves move theirs; in Collide, overlapping living bodies part; after Collide, each
    /// unit that walks stands within the bounds again.
    fn install(cx: &mut InstallCx<'_>) {
        let (world, schedule) = (&mut *cx.world, &mut *cx.schedule);
        world.insert_resource(BodyIndex::new(Shape::MAX_BOUND));
        StaticsDirty::install(world);
        let run = world.register_system(StaticTracking::track_static_bodies);
        world.insert_resource(StaticTrackingRun(run));
        let run = world.register_system(RoomMaking::make_room);
        world.insert_resource(RoomMakingRun(run));
        schedule.add_systems((
            StaticTracking::track_static_bodies.in_set(NavigationSet::TrackStatics),
            (
                Walking::forget_dead,
                Routing::route_units,
                Routing::plan_routes,
                Walking::steer,
                Walking::move_units,
                ForcedMoves::force_units,
            )
                .chain()
                .in_set(NavigationSet::Move),
            (StaticTracking::track_static_bodies, Walking::collide)
                .chain()
                .in_set(NavigationSet::Collide),
            Walking::keep_in_bounds.in_set(NavigationSet::Bounds),
        ));
    }
}

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

    /// Sets the destination of the unit of `entity`, which walks, to `target`, leaving one that
    /// does not change untouched.
    pub(crate) fn go_to(world: &mut World, entity: Entity, target: Option<Position>) {
        if let Some(mut destination) = world.get_mut::<Destination>(entity) {
            destination.set_if_neq(Destination::to(target));
        }
    }

    /// Gives the match the map's pathing grid on the cells of `grid`, those the walls of its
    /// navigation book block, for the kinds of `walkers`, a static index for the widest of them,
    /// and a planner of routes on the grid; the static bodies fill the grid and the index from
    /// the first tick on.
    pub(crate) fn load_pathing(
        world: &mut World,
        grid: Grid,
        walkers: impl IntoIterator<Item = Walker>,
    ) {
        let walkers: Vec<Walker> = walkers.into_iter().collect();
        debug_assert!(
            world.contains_resource::<StaticChanges>(),
            "a map's pathing grid is navigation's, which the load checked the mode declares"
        );
        let terrain = Terrain::new(&grid, world.resource::<NavigationBook>().walls.walls());
        let widest = walkers.iter().map(|walker| walker.radius).max();
        world.insert_resource(BodyIndex::new(widest.unwrap_or(Num::ZERO)));
        world.insert_resource(RoutePlanner::new(&grid));
        world.insert_resource(PathingGrid::new(grid, walkers, &terrain));
        world.resource_mut::<StaticsDirty>().set();
    }

    /// Makes room for the box of `entity`, which just spawned: each walker of its layer whose
    /// body the box overlaps goes at once to the nearest cell its walker may stand in, by stable
    /// id, as a teleport places a unit but with no disjoint. The static index and the pathing
    /// grid take the box first, so the cell is clear of every static body, and no push of
    /// collision can hold the walker between two. With no pathing grid, the box pushes it out
    /// as collision would.
    pub(crate) fn make_room(world: &mut World, entity: Entity) {
        let run = world.resource::<RoomMakingRun>().0;
        world
            .run_system_with(run, entity)
            .expect("a box makes room on any world");
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
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, StableId};

    use crate::books::Books;
    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::navigation::Navigation;
    use crate::navigation::destination::Destination;
    use crate::navigation::navigation_book::NavigationBook;
    use crate::navigation::paths::Paths;
    use crate::navigation::walker::Walker;
    use crate::navigation::wall::Wall;
    use crate::navigation::walls::Walls;
    use crate::units::Units;
    use crate::units::body::Body;
    use crate::units::move_step::MoveStep;
    use crate::units::team::Team;

    impl Navigation {
        /// Loads a pathing grid of `cell` cells from `min` to `max` for `walkers`.
        pub(crate) fn load_open_pathing(
            world: &mut World,
            cell: impl Into<Num>,
            min: [i64; 2],
            max: [i64; 2],
            walkers: impl IntoIterator<Item = Walker>,
        ) {
            let cell = cell.into();
            Navigation::load_walled(world, cell, min, max, walkers, &[]);
        }

        /// Loads a pathing grid as `load_pathing` does, with `walls`.
        pub(crate) fn load_walled(
            world: &mut World,
            cell: impl Into<Num>,
            min: [i64; 2],
            max: [i64; 2],
            walkers: impl IntoIterator<Item = Walker>,
            walls: &[Wall],
        ) {
            let cell = cell.into();
            let bounds = Bounds::new(min.map(Num::int), max.map(Num::int)).unwrap();
            let grid = Grid::new(cell, bounds).unwrap();
            Navigation::load_walls(world, walls);
            Navigation::load_pathing(world, grid, walkers);
        }

        /// Gives the match the walls `walls`, which a pathing grid loaded after blocks.
        pub(crate) fn load_walls(world: &mut World, walls: &[Wall]) {
            let book = NavigationBook {
                walls: Walls::new(walls),
                ..world.resource::<NavigationBook>().clone()
            };
            Books::load(world, book);
        }

        /// Gives the match the paths `paths`.
        pub(crate) fn load_paths(world: &mut World, paths: Paths) {
            let book = NavigationBook {
                paths,
                ..world.resource::<NavigationBook>().clone()
            };
            Books::load(world, book);
        }

        /// A unit at `pos` walking a meter a tick to `to`.
        pub(crate) fn unit(
            world: &mut World,
            pos: impl Into<Position>,
            to: Option<Position>,
        ) -> StableId {
            let pos = pos.into();
            let walker = Navigation::walker(MoveStep::new(Num::ONE).unwrap());
            let id = Units::spawn(world, pos, Team::new(0), walker);
            *world.part_mut::<Destination>(id) = Destination::to(to);
            id
        }

        /// A unit at `pos` with a body of `radius`, walking `step` a tick to `to`, or one that does not
        /// walk when `step` is `None`.
        pub(crate) fn body(
            world: &mut World,
            pos: impl Into<Position>,
            to: Option<Position>,
            step: Option<Num>,
            radius: impl Into<Num>,
        ) -> StableId {
            let pos = pos.into();
            let radius = radius.into();
            Navigation::body_on(world, pos, to, step, Body::new(radius).unwrap())
        }

        /// A unit at `pos` with `body`, walking `step` a tick to `to`, or one that does not walk when
        /// `step` is `None`.
        pub(crate) fn body_on(
            world: &mut World,
            pos: impl Into<Position>,
            to: Option<Position>,
            step: Option<Num>,
            body: Body,
        ) -> StableId {
            let pos = pos.into();
            let id = Units::spawn(world, pos, Team::new(0), body);
            if let Some(step) = step {
                world.insert_parts(id, Navigation::walker(MoveStep::new(step).unwrap()));
                *world.part_mut::<Destination>(id) = Destination::to(to);
            }
            id
        }
    }
}

#[cfg(test)]
mod tests;
