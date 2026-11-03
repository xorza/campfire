use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_common::Tick;
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_geometry::{Bounds, Polygon, Shape};
use campfire_math::{Num, Vec3};
use campfire_navigation::internals::PlannedRoute;
use campfire_navigation::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{
    Capability, MatchRate, Position, SimSystems, SimTick, SimWorld, StableId, Unpredicted,
};
use campfire_units::{
    ArrivedShort, AtWork, Body, BodyForm, BodyShape, Dead, Destination, Facing, Layer, MapBook,
    TurnStep, Units, WalkGoal, Wall,
};
use campfire_values::{Block, PathId, Team};

use crate::halves;

const ONE: i64 = 1 << 24;

/// The second layer, as an RTS's air.
const AIR: Layer = Layer::new(1);

/// A walker of `radius` on the first layer.
const fn ground(radius: Num) -> Walker {
    Walker {
        layer: Layer::FIRST,
        radius,
    }
}

/// A match of navigation, with the type its units are of.
fn walk_match() -> Match {
    walk_match_of(&[Capability::Navigation])
}

/// A match of `declared`, with navigation among them, as `walk_match` gives one.
fn walk_match_of(declared: &[Capability]) -> Match {
    let mut game = Match::builder(declared).role(Role::Rules).build();
    Navigation::walker_type(&mut game);
    game
}

/// A match of navigation and of stats, whose modifiers block units.
fn blocking_walk_match() -> Match {
    walk_match_of(&[Capability::Stats, Capability::Navigation])
}

/// The walls of the lane a wide walker passes its wide friends in: dead ends above it, none, or
/// none and an end at x = 6, past which the ground is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Alcoves,
    Closed,
    Open,
}

#[test]
fn every_navigation_type_is_state() {
    let mut game = walk_match();
    let id = Navigation::unit(&mut game, (0, 0, 0), Some(Vec3::from((0, 0, 5))));
    game.insert_parts(
        id,
        (
            PathWalker::start(PathEnd::Start, id),
            OnPath::new(PathId::new(0)),
        ),
    );
    let ask = RouteAsk {
        goal: WalkGoal::Point(Vec3::from((3, 0, 4))),
        tick: Tick::new(2),
        party: None,
    };
    Navigation::ask_route(&mut game, id, ask);
    let lane = || Paths::new([("lane", &[Vec3::from((0, 0, 0)), Vec3::from((0, 0, 5))][..])]);
    Navigation::load_paths(&mut game, lane());
    // A restore loads the map first, as the packages give it.
    let mut restored = walk_match();
    Navigation::load_paths(&mut restored, lane());
    Release::restore_into(&game, &mut restored);
}

mod boxes;
mod collision;
mod crowds;
mod flight;
mod forced;
mod groups;
mod routing;
mod steering;
mod zero_hour;
