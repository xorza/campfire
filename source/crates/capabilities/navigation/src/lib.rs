// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::In;
use bevy_ecs::world::World;
use campfire_actions::ActionsSystems;
use campfire_declarations::{
    ById, ClientForm, Declarations, InstallCx, Module, ModuleSystems, ServerForm, SetPlace,
};
use campfire_geometry::Grid;
use campfire_math::{Num, Vec3};
use campfire_script::{ApiVersion, EventPhase, Status};
use campfire_sim::{Capability, SimEdgeSystems, SimSystems};
use campfire_units::{
    BodyUnit, Dead, DeclaredTags, FlightGoal, Layer, MapBook, MoveStep, OnWings, OpenCellAsk,
    OpenCellRun, SpawnCallbacks, UnitTracker, UnitsSystems, WallBook,
};
use campfire_values::{EngineTag, PathId, TagProperties, UnitType};

use crate::crowd_costs::CrowdCosts;
use crate::fliers::Fliers;
use crate::movement_model::MovementModel;
use crate::navigation_column::RowReads;
use crate::pathing_grid::PathingGrid;
use crate::route_planner::Walkable;
use crate::terrain::WallGrid;
use crate::wings::Wings;

mod altitude;
mod arrived_at;
mod aside_depth;
#[cfg(feature = "internals")]
pub mod bench;
mod body_index;
mod broadphase;
mod collider;
mod crowd_book;
mod crowd_costs;
mod error;
mod fliers;
mod flight_data;
mod forced_moves;
mod gait;
mod group_box;
mod held_place;
mod hemmed;
mod locomotor_data;
mod map_navigation_data;
mod map_paths_data;
mod mode_navigation_data;
mod move_effect_data;
mod movement_model;
mod moves;
mod navigation_api;
mod navigation_book;
mod navigation_books;
mod navigation_column;
mod navigation_effect;
mod on_path;
mod party;
mod path_arrivals;
mod path_following;
mod path_walk;
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
mod stalled_walkers;
mod static_bodies;
mod static_tracking;
mod steering;
mod terrain;
mod unit_navigation_data;
mod unparted_pairs;
mod walk_check;
mod walker;
mod walker_book;
mod walker_kinds;
mod walker_on_path;
mod walking;
mod wings;
mod zero_hour;

pub use crate::altitude::Altitude;
pub use crate::arrived_at::ArrivedAt;
pub use crate::aside_depth::AsideDepth;
pub use crate::body_index::{BodyIndex, IndexedBody};
pub use crate::collider::Collider;
pub use crate::crowd_book::CrowdBook;
pub use crate::error::{MapProblem, NavigationDataError, NavigationRefusal};
pub use crate::flight_data::{FlightData, WingsData};
pub use crate::forced_moves::ForcedMoves;
pub use crate::gait::Gait;
pub use crate::group_box::GroupBox;
pub use crate::held_place::HeldPlace;
pub use crate::hemmed::Hemmed;
pub use crate::locomotor_data::LocomotorData;
pub use crate::map_navigation_data::{MapNavigationData, WallData};
pub use crate::map_paths_data::{MapPathsData, PathData};
pub use crate::mode_navigation_data::ModeNavigationData;
pub use crate::move_effect_data::MoveEffectData;
pub use crate::moves::Moves;
pub use crate::navigation_api::NavigationApi;
pub use crate::navigation_book::NavigationBook;
pub use crate::navigation_column::{NavigationColumn, RoomAsk, RoomSearch};
pub use crate::navigation_effect::NavigationEffect;
pub use crate::on_path::OnPath;
pub use crate::party::{Party, PartyKey, PartyOrder};
pub use crate::path_arrivals::{OnPathEnd, PathArrival, PathArrivals};
pub use crate::path_following::PathFollowing;
pub use crate::path_walk::PathWalk;
pub use crate::path_walker::{PathEnd, PathWalker};
pub use crate::paths::Paths;
pub use crate::progress::Progress;
pub use crate::room_making::{RoomMaking, RoomMakingRun};
pub use crate::route::{
    Route, RouteAsk, RouteCursor, RouteMut, RouteMutItem, RouteMutReadOnlyItem, RouteRead,
    RouteReadItem, RouteRef,
};
pub use crate::route_planner::RoutePlanner;
pub use crate::routing::Routing;
pub use crate::segment::Segment;
pub use crate::stalled_walkers::StalledWalkers;
pub use crate::static_bodies::StaticBodies;
pub use crate::static_tracking::{StaticTracking, StaticTrackingRun};
pub use crate::unit_navigation_data::{UnitNavigationData, UnitNavigationPatch};
pub use crate::unparted_pairs::UnpartedPairs;
pub use crate::walk_check::{MarkerPoint, PlacedBody, WalkCheck};
pub use crate::walker::Walker;
pub use crate::walker_book::WalkerBook;
pub use crate::walker_kinds::WalkerKinds;
pub use crate::walker_on_path::WalkerOnPath;
pub use crate::walking::Walking;
pub use crate::zero_hour::zero_hour_drive::ZeroHourDrive;
pub use crate::zero_hour::zero_hour_locomotor::{LocomotorAppearance, ZeroHourLocomotor};

/// Navigation's steps.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavigationSystems {
    /// The static index and the pathing grid take the static bodies as the tick starts.
    TrackStatics,
    /// Path walkers walk to their waypoints, before units chase their targets.
    FollowPaths,
    /// Units route and move.
    Move,
    /// The static bodies are taken again, walkers collide, the units out of the map's bounds come
    /// back in, and fliers fly.
    Collide,
    /// The static bodies are taken once the tick's last stage is done, so the index holds them
    /// as they stand at the boundary.
    TrackEnd,
}

impl ModuleSystems for NavigationSystems {
    const ALL: &'static [Self] = &[
        Self::TrackStatics,
        Self::FollowPaths,
        Self::Move,
        Self::Collide,
        Self::TrackEnd,
    ];

    fn place(self) -> SetPlace {
        match self {
            Self::TrackStatics => SetPlace::at(UnitsSystems::Begin),
            Self::FollowPaths => SetPlace::at(ActionsSystems::Chase),
            Self::Move => SetPlace::stage(SimSystems::Move),
            Self::Collide => SetPlace::stage(SimSystems::Collide),
            Self::TrackEnd => SetPlace::gap(SimEdgeSystems::After(SimSystems::Vision)),
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
    type Systems = NavigationSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ModeNavigationData>();
        d.section::<MapNavigationData>();
        d.section::<MapPathsData>();
        d.section::<UnitNavigationData>();
        d.section::<FlightData>();
        d.section::<LocomotorData>();
        d.engine_tags(&[Self::AIR_TARGET, OnWings::TAG]);
        d.removals::<Dead>();
        d.callback::<SpawnCallbacks>(Capability::Navigation, Self::make_room);
        d.component::<PathWalker>();
        d.component::<OnPath>();
        d.resource::<PathArrivals>();
        d.mode_events::<PathArrivals>(EventPhase::Arrivals);
        d.hook::<OnPathEnd>(Status::Runs(ApiVersion::FIRST));
        d.component::<Route>();
        d.component::<RouteCursor>();
        d.timed::<Route>();
        d.component::<Progress>();
        d.component::<ArrivedAt>();
        d.require::<MoveStep, ArrivedAt>();
        d.component::<AsideDepth>();
        d.require::<MoveStep, AsideDepth>();
        d.component::<Hemmed>();
        d.require::<MoveStep, Hemmed>();
        d.report::<UnpartedPairs>();
        d.report::<StalledWalkers>();
        d.cache::<UnitTracker<StaticBodies>>();
        d.cache::<BodyIndex>();
        d.cache::<PathingGrid>();
        d.cache::<RoutePlanner>();
        d.cache::<CrowdCosts>();
        d.observer(UnitTracker::<StaticBodies>::aged);
        d.observer(UnitTracker::<StaticBodies>::disabled);
        d.observer(UnitTracker::<StaticBodies>::enabled);
        d.observer(StaticBodies::part_removed);
        d.observer(StaticBodies::state_inserted);
        d.observer(StaticBodies::state_removed);
        d.observer(Route::rewritten);
        d.one_shot::<StaticTrackingRun, _>(StaticTracking::track_static_bodies);
        d.one_shot::<RoomMakingRun, _>(RoomMaking::make_room);
        d.one_shot::<OpenCellRun, _>(Self::open_cell_of);
        d.listed::<MoveEffectData>();
        d.book::<ById<UnitType, Walker>>(Walker::book);
        d.book::<WalkerBook>(WalkerBook::build);
        d.book::<ById<UnitType, UnitNavigationData>>(UnitNavigationData::book);
        d.book::<ById<UnitType, FlightData>>(FlightData::book);
        d.book::<ById<UnitType, MovementModel>>(MovementModel::book);
        d.component::<ZeroHourDrive>();
        d.component::<Altitude>();
        d.component::<HeldPlace>();
        d.component::<FlightGoal>();
        d.book::<CrowdBook>(CrowdBook::build);
        d.names::<Layer>(Self::layer_names);
        d.extend::<DeclaredTags>(Self::layer_tags);
        d.book::<NavigationBook>(NavigationBook::build);
        d.names::<PathId>(NavigationBook::path_names);
        d.book::<WallBook>(Self::walls);
        d.rule(Self::pathing_rule);
        d.column::<NavigationColumn, RowReads>(NavigationColumn::fill_row);
        d.api(NavigationApi::register);
        // A unit that walks, which its move step tells, takes a route and a progress.
        d.part::<BodyUnit, Route>(|at| at.of_type::<MoveStep>().map(|_| Route::default()));
        d.part::<BodyUnit, Progress>(|at| at.of_type::<MoveStep>().map(|_| Progress::default()));
        d.part::<BodyUnit, ArrivedAt>(|at| at.of_type::<MoveStep>().map(|_| ArrivedAt::default()));
        d.part::<BodyUnit, AsideDepth>(|at| {
            at.of_type::<MoveStep>().map(|_| AsideDepth::default())
        });
        d.part::<BodyUnit, Hemmed>(|at| at.of_type::<MoveStep>().map(|_| Hemmed::default()));
        // A walker of a model that keeps a velocity drives from a stop
        // (04-capabilities/navigation.md, A mover keeps a velocity).
        d.part::<BodyUnit, ZeroHourDrive>(|at| {
            at.of_type::<MoveStep>()?;
            let model = at.of_type::<MovementModel>()?;
            matches!(model, MovementModel::ZeroHour(_)).then(ZeroHourDrive::default)
        });
        // A flier, which its type's flight tells, flies from its height in the world as it spawns.
        d.part::<BodyUnit, Altitude>(|at| {
            at.of_type::<FlightData>()?;
            let position = at.position()?;
            let map = at.world().get_resource::<MapBook>();
            let point = map.map_or(position, |map| map.point(position));
            Some(Altitude::new(point.y))
        });
        // A flier flies to its height until the capability that parks it holds it on the ground
        // (04-capabilities/airfields.md, A jet takes off and lands on its runway).
        d.part::<BodyUnit, FlightGoal>(|at| at.of_type::<FlightData>().map(|_| FlightGoal::Height));
        // A flier on wings holds the place it spawns at (04-capabilities/navigation.md, A jet flies
        // on wings).
        d.part::<BodyUnit, HeldPlace>(|at| {
            at.of_type::<FlightData>()?.wings?;
            Some(HeldPlace::new(at.position()?))
        });
    }

    /// Adds navigation to a match, with no paths, the world for bounds, and a static index for
    /// walkers as wide as a body may be, until the mode sets its map's: in Act, before units chase
    /// their targets, path walkers walk their paths; in Move, units walk their routes to their
    /// destinations, straight lines until the map gives a pathing grid, winged fliers in the air
    /// fly, then the forced moves move theirs; in Collide, overlapping living bodies part, each
    /// unit that walks stands within the bounds again, and fliers fly to their heights.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.add(
            NavigationSystems::TrackStatics,
            StaticTracking::track_static_bodies,
        );
        cx.by_role(
            NavigationSystems::FollowPaths,
            PathFollowing::follow_paths::<ServerForm>,
            PathFollowing::follow_paths::<ClientForm>,
        );
        cx.by_role(
            NavigationSystems::Move,
            (
                Walking::forget_dead,
                Walking::price_crowds,
                Routing::route_units,
                Routing::plan_routes,
                Walking::steer::<ServerForm>,
                Walking::move_units,
                Wings::fly,
                ForcedMoves::force_units,
            )
                .chain(),
            (
                Walking::forget_dead,
                Walking::price_crowds,
                Routing::route_units,
                Routing::plan_routes,
                Walking::steer::<ClientForm>,
                Walking::move_units,
                Wings::fly,
                ForcedMoves::force_units,
            )
                .chain(),
        );
        cx.add(
            NavigationSystems::Collide,
            (
                StaticTracking::track_static_bodies,
                Walking::collide,
                Walking::keep_in_bounds,
                Fliers::fly,
            )
                .chain(),
        );
        cx.add(
            NavigationSystems::TrackEnd,
            StaticTracking::track_static_bodies,
        );
    }
}

impl Navigation {
    /// The engine tag of a flier whose height above the ground passes its type's airborne height
    /// (04-capabilities/stats.md, Airborne is a tag of height).
    pub const AIR_TARGET: EngineTag = EngineTag::new(2, "air_target", TagProperties::NONE);

    /// Gives `world` its map's pathing grid, when the map has one, for the kinds of walker its
    /// book holds.
    pub fn load_map(world: &mut World) {
        let grid = world.get_resource::<NavigationBook>();
        if let Some(grid) = grid.and_then(|book| book.pathing) {
            let walkers = world.resource::<WalkerBook>().kinds.clone();
            Self::load_pathing(world, grid, walkers);
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
        let terrain = WallGrid::new(&grid, world.resource::<WallBook>().walls());
        let widest = walkers.iter().map(|walker| walker.radius).max();
        world.insert_resource(BodyIndex::new(widest.unwrap_or(Num::ZERO)));
        world.insert_resource(RoutePlanner::new(&grid));
        world.insert_resource(CrowdCosts::new(grid, &walkers));
        world.insert_resource(PathingGrid::new(grid, walkers, &terrain));
        world.resource_mut::<UnitTracker<StaticBodies>>().mark();
    }

    /// The spawn callback that makes room for the body of `entity`, which the mode's spawner just
    /// spawned, when it stands, a box or a circle: each walker of its layer whose body it overlaps
    /// goes at once to the nearest cell its walker may stand in, by stable id, as a teleport
    /// places a unit but with no disjoint. The static index and the pathing grid take the body
    /// first, whether it overlaps a walker or not, so the cell is clear of every static body, no
    /// push of collision can hold the walker between two, and a spawn its producer sends at once
    /// stands clear of it. With no pathing grid, the body pushes it out as collision would.
    fn make_room(world: &mut World, entity: Entity) {
        let run = world.resource::<RoomMakingRun>().0;
        world
            .run_system_with(run, entity)
            .expect("a static body makes room on any world");
    }

    /// The center of the cell nearest `ask.near` among those its type's walker may stand in, as
    /// `open_cell` gives it; `None` for a type that does not walk.
    fn open_cell_of(In(ask): In<OpenCellAsk>, world: &World) -> Option<Vec3> {
        let walker = *world
            .resource::<ById<UnitType, Walker>>()
            .get(ask.unit_type)?;
        Self::open_cell(world, walker, ask.near)
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
    pub use crate::route::internals::PlannedRoute;

    use bevy_ecs::bundle::{Bundle, NoBundleEffect};
    use bevy_ecs::world::World;

    use campfire_declarations::ById;
    use campfire_declarations::internals::BookLoad;
    use campfire_geometry::{Bounds, Grid};
    use campfire_math::{Num, Vec3};
    use campfire_sim::{MatchRate, SimWorld, StableId, UnitDues};
    use campfire_units::{Destination, Leaving, MoveStep, Wall, WallBook};
    use campfire_values::UnitType;

    use crate::Navigation;
    use crate::arrived_at::ArrivedAt;
    use crate::aside_depth::AsideDepth;
    pub use crate::broadphase::internals::scene;
    use crate::crowd_book::CrowdBook;
    use crate::flight_data::FlightData;
    use crate::hemmed::Hemmed;
    use crate::movement_model::MovementModel;
    use crate::moves::Moves;
    use crate::navigation_book::NavigationBook;
    use crate::pathing_grid::PathingGrid;
    use crate::paths::Paths;
    use crate::progress::Progress;
    use crate::route::{Route, RouteAsk, RouteCursor, RouteMut, RouteRef};
    use crate::unit_navigation_data::UnitNavigationData;
    use crate::walker::Walker;
    use crate::walker_kinds::WalkerKinds;
    use crate::zero_hour::zero_hour_locomotor::ZeroHourLocomotor;
    use crate::zero_hour::zero_hour_mover::ZeroHourMover;

    impl Navigation {
        /// The components of a new unit that walks `step` a tick, with nowhere to go yet.
        pub fn walker(step: MoveStep) -> impl Bundle<Effect: NoBundleEffect> {
            (
                step,
                Destination::default(),
                Leaving::default(),
                Route::default(),
                Progress::default(),
                ArrivedAt::default(),
                AsideDepth::default(),
                Hemmed::default(),
            )
        }

        /// The waypoints the walker `id` has yet to reach, the next first.
        pub fn ahead(world: &World, id: StableId) -> &[Vec3] {
            let entity = world.entity_of(id).expect("a walker of the match");
            let unit = world.entity(entity);
            let route = unit.get::<Route>().expect("a walker has a route");
            let cursor = unit.get::<RouteCursor>().expect("a route has its cursor");
            RouteRef::new(route, cursor).ahead()
        }

        /// Asks the route `ask` names for the walker `id`, with its progress.
        pub fn ask_route(world: &mut World, id: StableId, ask: RouteAsk) {
            let entity = world.entity_of(id).expect("a walker of the match");
            UnitDues::<Route>::with(world, id, entity, |world, dues| {
                let mut unit = world.entity_mut(entity);
                let (mut route, mut progress) = unit
                    .get_components_mut::<(RouteMut, &mut Progress)>()
                    .expect("a walker has a route and its progress");
                route.ask(ask, &mut progress, dues);
            });
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
            WallBook::new(walls).load_into(world);
        }

        /// Gives the match the paths `paths`.
        pub fn load_paths(world: &mut World, paths: Paths) {
            let book = NavigationBook {
                paths,
                ..world.resource::<NavigationBook>().clone()
            };
            book.load_into(world);
        }

        /// Gives the match the rule that a walker moves the units `moves` names, as its mode's
        /// navigation would.
        pub fn load_moves(world: &mut World, moves: Moves) {
            CrowdBook { moves }.load_into(world);
        }

        /// Gives the units of `unit_type` the flight `flight`, as their type's section would; units
        /// spawned after fly.
        pub fn load_flight(world: &mut World, unit_type: UnitType, flight: FlightData) {
            ById::<UnitType, FlightData>::amend(world, |_, book| {
                book.set(unit_type, flight);
            });
        }

        /// Gives the units of `unit_type` the Zero Hour locomotor `locomotor` at the match's rate,
        /// as their type's section would; units spawned after drive by it.
        pub fn load_locomotor(
            world: &mut World,
            unit_type: UnitType,
            locomotor: ZeroHourLocomotor,
        ) {
            let rate = *world.resource::<MatchRate>();
            ById::<UnitType, MovementModel>::amend(world, |_, book| {
                book.set(
                    unit_type,
                    MovementModel::ZeroHour(ZeroHourMover::of(&locomotor, rate)),
                );
            });
        }

        /// Gives the units of `unit_type` `navigation`, as their type's section would.
        pub fn load_unit_navigation(
            world: &mut World,
            unit_type: UnitType,
            navigation: UnitNavigationData,
        ) {
            ById::<UnitType, UnitNavigationData>::amend(world, |_, book| {
                book.set(unit_type, navigation);
            });
        }
    }
}
