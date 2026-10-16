#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::EffectQueues;
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_geometry::Grid;
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, SimEdge, SimSet, Unpredicted};
use campfire_units::{BodyUnit, Dead, Destination, MoveStep, SpawnHooks, UnitTracker};

use crate::navigation_column::RowReads;
use crate::pathing_grid::PathingGrid;
use crate::route_planner::Walkable;
use crate::terrain::Terrain;

#[cfg(feature = "internals")]
pub mod bench;
mod body_index;
mod broadphase;
mod collider;
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
mod static_bodies;
mod static_tracking;
mod steering;
mod terrain;
mod walk_check;
mod walker;
mod walker_kinds;
mod walking;
mod wall;
mod walls;

pub use crate::body_index::{BodyIndex, IndexedBody};
pub use crate::collider::Collider;
pub use crate::error::{MapProblem, NavigationRefusal};
pub use crate::forced_moves::ForcedMoves;
pub use crate::group_box::GroupBox;
pub use crate::mode_navigation_data::ModeNavigationData;
pub use crate::navigation_api::NavigationApi;
pub use crate::navigation_book::NavigationBook;
pub use crate::navigation_column::NavigationColumn;
pub use crate::navigation_effect::NavigationEffect;
pub use crate::on_path::OnPath;
pub use crate::party::{Party, PartyKey, PartyOrder};
pub use crate::path_walker::{PathEnd, PathWalker};
pub use crate::paths::Paths;
pub use crate::progress::Progress;
pub use crate::room_making::{RoomMaking, RoomMakingRun};
pub use crate::route::{Route, RouteAsk};
pub use crate::route_planner::RoutePlanner;
pub use crate::routing::Routing;
pub use crate::segment::Segment;
pub use crate::static_bodies::StaticBodies;
pub use crate::static_tracking::{StaticTracking, StaticTrackingRun};
pub use crate::walk_check::{MarkerPoint, PlacedBody, WalkCheck};
pub use crate::walker::Walker;
pub use crate::walker_kinds::WalkerKinds;
pub use crate::walking::Walking;
pub use crate::wall::Wall;
pub use crate::walls::Walls;

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
    /// The static bodies are taken once the tick's last stage is done, so the index holds them
    /// as they stand at the boundary.
    TrackEnd,
    /// Each walker's `ArrivedShort` follows its route, in a gap, so a capability that reads it
    /// in a stage reads the route as the stage began.
    Report(SimEdge),
}

impl NavigationSet {
    /// The steps in the stages and after Collide.
    const STEPS: [Self; 5] = [
        Self::TrackStatics,
        Self::Move,
        Self::Collide,
        Self::Bounds,
        Self::TrackEnd,
    ];

    /// The steps, then a report in each gap, `SimEdge::Start` first.
    const ALL: [Self; Self::STEPS.len() + 1 + SimSet::ALL.len()] = {
        let steps = Self::STEPS.len();
        let mut all = [Self::Report(SimEdge::Start); Self::STEPS.len() + 1 + SimSet::ALL.len()];
        let mut at = 0;
        while at < steps {
            all[at] = Self::STEPS[at];
            at += 1;
        }
        let mut stage = 0;
        while stage < SimSet::ALL.len() {
            all[steps + 1 + stage] = Self::Report(SimEdge::After(SimSet::ALL[stage]));
            stage += 1;
        }
        all
    };
}

impl ModuleSet for NavigationSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Navigation);
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::TrackStatics => SetPlace::Phase(Phase::Begin),
            Self::Move => SetPlace::Phase(Phase::Move),
            Self::Collide => SetPlace::Phase(Phase::Collide),
            Self::Bounds => SetPlace::Gap(SimEdge::After(SimSet::Collide)),
            Self::TrackEnd => SetPlace::Gap(SimEdge::After(SimSet::Vision)),
            Self::Report(edge) => SetPlace::Gap(edge),
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
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<NavigationSet>();
        d.removals::<Dead>();
        d.removals::<Unpredicted>();
        d.callback::<SpawnHooks>(Capability::Navigation, Self::make_room);
        d.component::<PathWalker>();
        d.component::<OnPath>();
        d.component::<Route>();
        d.component::<Progress>();
        d.cache::<UnitTracker<StaticBodies>>();
        d.cache::<BodyIndex>();
        d.cache::<PathingGrid>();
        d.cache::<RoutePlanner>();
        d.observer(UnitTracker::<StaticBodies>::aged);
        d.observer(StaticBodies::part_removed);
        d.observer(StaticBodies::state_inserted);
        d.observer(StaticBodies::state_removed);
        d.one_shot::<StaticTrackingRun, _>(StaticTracking::track_static_bodies);
        d.one_shot::<RoomMakingRun, _>(RoomMaking::make_room);
        d.callback::<EffectQueues>(Capability::Navigation, NavigationEffect::queue_listed);
        d.book::<ByType<Walker>>();
        d.book::<NavigationBook>();
        d.column::<NavigationColumn, RowReads>(NavigationColumn::fill_row);
        d.api(NavigationApi::register);
        // A unit that walks, which its move step tells, takes a route and a progress.
        d.part::<BodyUnit, Route>(|at| at.of_type::<MoveStep>().map(|_| Route::default()));
        d.part::<BodyUnit, Progress>(|at| at.of_type::<MoveStep>().map(|_| Progress::default()));
    }

    /// Adds navigation to a match, with no paths, the world for bounds, and a static index for
    /// walkers as wide as a body may be, until the mode sets its map's: in Move, units walk their
    /// routes to their destinations, straight lines until the map gives a pathing grid, then the
    /// forced moves move theirs; in Collide, overlapping living bodies part; after Collide, each
    /// unit that walks stands within the bounds again.
    fn install(cx: &mut InstallCx<'_>) {
        cx.systems((
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
            StaticTracking::track_static_bodies
                .in_set(NavigationSet::TrackEnd)
                .before(NavigationSet::Report(SimEdge::After(SimSet::Vision))),
        ));
        for set in NavigationSet::ALL {
            if let NavigationSet::Report(_) = set {
                cx.systems(Walking::report_short.in_set(set));
            }
        }
    }
}

impl Navigation {
    /// Sets the destination of the unit of `entity`, which walks, to `target`, leaving one that
    /// does not change untouched.
    pub fn go_to(world: &mut World, entity: Entity, target: Option<Vec3>) {
        if let Some(mut destination) = world.get_mut::<Destination>(entity) {
            destination.set_if_neq(Destination::to(target));
        }
    }

    /// Gives the match the map's pathing grid on the cells of `grid`, those the walls of its
    /// navigation book block, for the kinds of `walkers`, a static index for the widest of them,
    /// and a planner of routes on the grid; the static bodies fill the grid and the index from
    /// the first tick on.
    pub fn load_pathing(world: &mut World, grid: Grid, walkers: impl Into<WalkerKinds>) {
        let walkers = walkers.into();
        debug_assert!(
            world.contains_resource::<BodyIndex>(),
            "a map's pathing grid is navigation's, which the load checked the mode declares"
        );
        let terrain = Terrain::new(&grid, world.resource::<NavigationBook>().walls.walls());
        let widest = walkers.iter().map(|walker| walker.radius).max();
        world.insert_resource(BodyIndex::new(widest.unwrap_or(Num::ZERO)));
        world.insert_resource(RoutePlanner::new(&grid));
        world.insert_resource(PathingGrid::new(grid, walkers, &terrain));
        world.resource_mut::<UnitTracker<StaticBodies>>().mark();
    }

    /// The spawn hook that makes room for the body of `entity`, which the mode's spawner just
    /// spawned, when a box: each walker of its layer whose body the box overlaps goes at once to
    /// the nearest cell its walker may stand in, by stable id, as a teleport places a unit but
    /// with no disjoint. The static index and the pathing grid take the box first, so the cell
    /// is clear of every static body, and no push of collision can hold the walker between two.
    /// With no pathing grid, the box pushes it out as collision would.
    fn make_room(world: &mut World, entity: Entity) {
        let run = world.resource::<RoomMakingRun>().0;
        world
            .run_system_with(run, entity)
            .expect("a box makes room on any world");
    }

    /// The center of the cell nearest `point` among those `walker` may stand in, at `point`'s
    /// height, as a teleport finds its cell, with the static bodies as the tick began; `None`
    /// with no pathing grid, or on a grid with no such cell.
    pub fn open_cell(world: &World, walker: Walker, point: Vec3) -> Option<Vec3> {
        let grid = world.get_resource::<PathingGrid>()?;
        let planner = world.get_resource::<RoutePlanner>()?;
        let clearance = grid.serving(walker)?;
        let walkable = Walkable::of(clearance, world.resource::<BodyIndex>());
        let cell = planner.nearest_open(walkable, point, &mut 0)?;
        Some(clearance.grid().center(cell, point.y))
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_declare::Book;
    use campfire_geometry::{Bounds, Grid};
    use campfire_math::Num;
    use campfire_sim::{SimWorld, StableId};
    use campfire_units::{Destination, MoveStep};

    use crate::Navigation;
    pub use crate::broadphase::internals::scene;
    use crate::navigation_book::NavigationBook;
    use crate::pathing_grid::PathingGrid;
    use crate::paths::Paths;
    use crate::progress::Progress;
    use crate::route::{Route, RouteAsk};
    use crate::walker::Walker;
    use crate::walker_kinds::WalkerKinds;
    use crate::wall::Wall;
    use crate::walls::Walls;

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

        /// Asks the route `ask` names for the walker `id`, with its progress.
        pub fn ask_route(world: &mut World, id: StableId, ask: RouteAsk) {
            let entity = world.entity_of(id).expect("a walker of the match");
            let mut unit = world.entity_mut(entity);
            let (mut route, mut progress) = unit
                .get_components_mut::<(&mut Route, &mut Progress)>()
                .expect("a walker has a route and its progress");
            route.ask(ask, &mut progress);
        }

        /// Whether the pathing grid of `world` holds the cell at `cell` open to `walker`.
        pub fn cell_open(world: &World, walker: Walker, cell: usize) -> bool {
            world.resource::<PathingGrid>().clearance(walker).open(cell)
        }

        /// Loads a pathing grid of `cell` cells from `min` to `max` for `walkers`.
        pub fn load_open_pathing(
            world: &mut World,
            cell: impl Into<Num>,
            min: [i64; 2],
            max: [i64; 2],
            walkers: impl IntoIterator<Item = Walker>,
        ) {
            let cell = cell.into();
            Self::load_walled(world, cell, min, max, walkers, &[]);
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
            Self::load_walls(world, walls);
            Self::load_pathing(world, grid, walkers.into_iter().collect::<WalkerKinds>());
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
