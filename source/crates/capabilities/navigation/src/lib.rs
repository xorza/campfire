#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::EffectQueues;
use campfire_core::{
    Body, BodyUnit, ByType, Declarations, InstallCx, Module, ModuleSet, MoveStep, Phase, SetPlace,
    SpawnHooks,
};
use campfire_geometry::{Grid, Shape};
use campfire_math::Num;
use campfire_sim::{Capability, Position, SimEdge, SimSet};

#[cfg(feature = "internals")]
pub mod bench;
mod body_index;
mod broadphase;
mod collider;
mod destination;
mod error;
mod forced_moves;
mod group_box;
mod mode_navigation_data;
mod navigation_api;
mod navigation_book;
mod navigation_column;
mod navigation_effect;
mod on_path;
mod party;
mod path_walker;
mod pathing_grid;
mod paths;
mod progress;
mod regions;
mod room_making;
mod route;
mod route_asks;
mod route_planner;
mod routing;
mod segment;
mod static_changes;
mod static_tracking;
mod statics_dirty;
mod steering;
mod terrain;
mod walker;
mod walking;
mod wall;
mod walls;

pub use crate::body_index::{BodyIndex, IndexedBody};
pub use crate::collider::Collider;
pub use crate::destination::Destination;
pub use crate::error::MapProblem;
pub use crate::forced_moves::ForcedMoves;
pub use crate::group_box::GroupBox;
pub use crate::mode_navigation_data::ModeNavigationData;
pub use crate::navigation_api::NavigationApi;
pub use crate::navigation_book::NavigationBook;
pub use crate::navigation_column::{NavigationColumn, RowReads};
pub use crate::navigation_effect::NavigationEffect;
pub use crate::on_path::OnPath;
pub use crate::party::{Party, PartyKey, PartyOrder};
pub use crate::path_walker::{PathEnd, PathWalker};
pub use crate::pathing_grid::{Clearance, PathingGrid};
pub use crate::paths::Paths;
pub use crate::progress::Progress;
pub use crate::regions::{Reach, Regions};
pub use crate::room_making::{RoomMaking, RoomMakingRun};
pub use crate::route::Route;
pub use crate::route_planner::{RoutePlanner, Walkable, Window};
pub use crate::routing::Routing;
pub use crate::segment::Segment;
pub use crate::static_changes::StaticChanges;
pub use crate::static_tracking::{StaticTracking, StaticTrackingRun};
pub use crate::statics_dirty::StaticsDirty;
pub use crate::terrain::Terrain;
pub use crate::walker::Walker;
pub use crate::walking::Walking;
pub use crate::wall::Wall;
pub use crate::walls::Walls;

campfire_common::enum_all! {
    /// Navigation's steps.
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
    pub const ALL;
}

impl ModuleSet for NavigationSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Navigation);
    const ALL: &'static [NavigationSet] = &NavigationSet::ALL;

    fn place(self) -> SetPlace {
        match self {
            NavigationSet::TrackStatics => SetPlace::Phase(Phase::Begin),
            NavigationSet::Move => SetPlace::Phase(Phase::Move),
            NavigationSet::Collide => SetPlace::Phase(Phase::Collide),
            NavigationSet::Bounds => SetPlace::Gap(SimEdge::After(SimSet::Collide)),
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
        d.callback::<SpawnHooks>(Capability::Navigation, Navigation::make_room);
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
    pub fn go_to(world: &mut World, entity: Entity, target: Option<Position>) {
        if let Some(mut destination) = world.get_mut::<Destination>(entity) {
            destination.set_if_neq(Destination::to(target));
        }
    }

    /// Gives the match the map's pathing grid on the cells of `grid`, those the walls of its
    /// navigation book block, for the kinds of `walkers`, a static index for the widest of them,
    /// and a planner of routes on the grid; the static bodies fill the grid and the index from
    /// the first tick on.
    pub fn load_pathing(world: &mut World, grid: Grid, walkers: impl IntoIterator<Item = Walker>) {
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

    /// The spawn hook that makes room for the body of `entity`, which the mode's spawner just
    /// spawned, when a box: each walker of its layer whose body the box overlaps goes at once to
    /// the nearest cell its walker may stand in, by stable id, as a teleport places a unit but
    /// with no disjoint. The static index and the pathing grid take the box first, so the cell
    /// is clear of every static body, and no push of collision can hold the walker between two.
    /// With no pathing grid, the box pushes it out as collision would.
    fn make_room(world: &mut World, entity: Entity) {
        let boxed = world
            .get::<Body>(entity)
            .is_some_and(|body| body.half_edges().is_some());
        if !boxed {
            return;
        }
        let run = world.resource::<RoomMakingRun>().0;
        world
            .run_system_with(run, entity)
            .expect("a box makes room on any world");
    }

    /// The center of the cell nearest `point` among those `walker` may stand in, at `point`'s
    /// height, as a teleport finds its cell, with the static bodies as the tick began; `None`
    /// with no pathing grid, or on a grid with no such cell.
    pub fn open_cell(world: &World, walker: Walker, point: Position) -> Option<Position> {
        let grid = world.get_resource::<PathingGrid>()?;
        let planner = world.get_resource::<RoutePlanner>()?;
        let clearance = grid.serving(walker)?;
        let walkable = Walkable::of(clearance, world.resource::<BodyIndex>());
        let cell = planner.nearest_open(walkable, point, &mut 0)?;
        Some(clearance.grid().center(cell, point.get().y))
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_core::Book;
    use campfire_geometry::{Bounds, Grid};
    use campfire_math::Num;

    use crate::Navigation;
    pub use crate::broadphase::internals::scene;
    use crate::navigation_book::NavigationBook;
    use crate::paths::Paths;
    use crate::walker::Walker;
    use crate::wall::Wall;
    use crate::walls::Walls;

    impl Navigation {
        /// Loads a pathing grid of `cell` cells from `min` to `max` for `walkers`.
        pub fn load_open_pathing(
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
        pub fn load_walled(
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
        pub fn load_walls(world: &mut World, walls: &[Wall]) {
            let book = NavigationBook {
                walls: Walls::new(walls),
                ..world.resource::<NavigationBook>().clone()
            };
            book.load_into(world);
        }

        /// Gives the match the paths `paths`.
        pub fn load_paths(world: &mut World, paths: Paths) {
            let book = NavigationBook {
                paths,
                ..world.resource::<NavigationBook>().clone()
            };
            book.load_into(world);
        }
    }
}
