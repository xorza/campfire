//! The idle scan of `orders` (04-capabilities/control.md, An idle unit scans for a target, as
//! its type declares).

use campfire_actions::ActionProgress;
use campfire_capabilities::Match;
use campfire_combat::Combat;
use campfire_declarations::Role;
use campfire_math::Vec3;
use campfire_orders::Orders;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Position, StableId};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{Destination, MoveStep, SeenBy, Units};
use campfire_values::{Relation, Team, TeamSet, UnitType};
use campfire_vision::Vision;

use crate::combatant;
use crate::orders::{ORDERS, armed, meter, orders_match, still_target};

/// A scan each 100 ms: at the tests' 30 ticks a second, ⌈100 · 30 / 1000⌉ = 3 ticks.
const SCAN_MS: u64 = 100;

/// A match's scanner type, whose one weapon reaches 2 m and which walks `step` a tick when given,
/// scanning each `SCAN_MS` for the units of `targets`, and a still target type.
#[derive(Debug)]
struct Scene {
    game: Match,
    scanner: UnitType,
    dummy: UnitType,
}

impl Scene {
    fn of(game: Match, targets: &[&str], step: Option<MoveStep>) -> Self {
        let mut game = game;
        let scanner = armed(&mut game, combatant(100, 2, 1, 30, 10), step);
        Orders::load_acquire(&mut game, scanner, SCAN_MS, targets);
        let dummy = armed(&mut game, still_target(100), None);
        Self {
            game,
            scanner,
            dummy,
        }
    }

    fn new(targets: &[&str], step: Option<MoveStep>) -> Self {
        Self::of(orders_match(), targets, step)
    }

    fn spawn(&mut self, unit_type: UnitType, at: (i32, i32, i32), team: u8) -> StableId {
        Spawn::of(unit_type, at)
            .team(Team::new(team))
            .place(&mut self.game)
    }

    fn scanner(&mut self, at: (i32, i32, i32)) -> StableId {
        self.spawn(self.scanner, at, 0)
    }

    fn dummy(&mut self, at: (i32, i32, i32), team: u8) -> StableId {
        self.spawn(self.dummy, at, team)
    }

    fn target(&self, id: StableId) -> Option<StableId> {
        self.game.part::<ActionProgress>(id).attack_target()
    }

    fn tick(&mut self) {
        self.game.step_with(&[]);
    }
}

/// Each idle unit scans in its turn: unit 0 in tick 0, as 0 % 3 = 0, and unit 1 in tick 1. Unit 0
/// takes the lower stable id of two enemies at 2 m, its weapon's reach; unit 1 the enemy at 1 m
/// over one at 2 m, and passes over an ally at 1 m and an enemy at 3 m, past its reach. A unit
/// with a target scans no other in its next turns, ticks 3 and 4.
#[test]
fn each_idle_unit_scans_in_its_turn_and_takes_the_nearest_target_in_reach() {
    let mut scene = Scene::new(&["enemies"], None);
    let first = scene.scanner((0, 0, 0));
    let second = scene.scanner((20, 0, 0));
    let tied = scene.dummy((2, 0, 0), 1);
    scene.dummy((0, 0, 2), 1);
    scene.dummy((22, 0, 0), 1);
    let near = scene.dummy((21, 0, 0), 1);
    scene.dummy((20, 0, 3), 1);
    scene.dummy((20, 0, 1), 0);
    assert_eq!([first.get(), second.get(), tied.get()], [0, 1, 2]);
    assert_eq!([scene.target(first), scene.target(second)], [None, None]);
    scene.tick();
    assert_eq!(
        [scene.target(first), scene.target(second)],
        [Some(tied), None]
    );
    scene.tick();
    assert_eq!(scene.target(second), Some(near));
    for _ in 2..5 {
        scene.tick();
    }
    assert_eq!(
        [scene.target(first), scene.target(second)],
        [Some(tied), Some(near)]
    );
}

/// A scan takes what a filter selects and its team sees: of a neutral and an unseen hostile at
/// 1 m and a seen hostile at 2 m, `hostiles` takes the hostile at 2 m, `enemies` the neutral,
/// and once team 0 sees the hostile at 1 m, `hostiles` takes it.
#[test]
fn a_scan_takes_what_its_filters_select_and_its_team_sees() {
    let cases = [
        ("hostiles", false, 3),
        ("enemies", false, 1),
        ("hostiles", true, 2),
    ];
    for (filter, seen, expected) in cases {
        let mut scene = Scene::new(&[filter], None);
        Vision::fog_grid(&mut scene.game, 3);
        Units::set_relation(
            &mut scene.game,
            Team::new(0),
            Team::new(2),
            Relation::Neutral,
        );
        let scanner = scene.scanner((0, 0, 0));
        let neutral = scene.dummy((1, 0, 0), 2);
        let hidden = scene.dummy((0, 0, 1), 1);
        let far = scene.dummy((2, 0, 0), 1);
        let team_zero = SeenBy::new(TeamSet::of(Team::new(0)));
        scene.game.insert_parts(neutral, team_zero);
        scene.game.insert_parts(far, team_zero);
        let hidden_seen = if seen {
            team_zero
        } else {
            SeenBy::new(TeamSet::NONE)
        };
        scene.game.insert_parts(hidden, hidden_seen);
        scene.tick();
        assert_eq!(
            scene.target(scanner).map(StableId::get),
            Some(expected),
            "{filter} {seen}"
        );
    }
}

/// A scanned target that leaves the reach is dropped, with no chase. The scan of tick 0 starts an
/// attack of a windup of a tick, which strikes in tick 1 though the target left, as range counts
/// only at the start; tick 2 drops the target, the scanner stands at the origin, and its next
/// scan, in tick 3, finds nothing within 2 m of the target at 5 m. An ordered target at 5 m is
/// chased, a meter a tick.
#[test]
fn a_scanned_target_out_of_reach_is_dropped_and_an_ordered_one_chased() {
    let mut scene = Scene::new(&["enemies"], Some(meter()));
    let scanner = scene.scanner((0, 0, 0));
    let dummy = scene.dummy((2, 0, 0), 1);
    scene.tick();
    assert_eq!(scene.target(scanner), Some(dummy));
    *scene.game.part_mut::<Position>(dummy) = Position::from(Vec3::from((5, 0, 0)));
    scene.tick();
    assert_eq!(Stats::health(&scene.game, dummy), 90);
    assert_eq!(scene.target(scanner), Some(dummy));
    for _ in 2..4 {
        scene.tick();
        assert_eq!(scene.target(scanner), None);
        assert_eq!(scene.game.part::<Position>(scanner).get(), Vec3::ZERO);
        assert_eq!(scene.game.part::<Destination>(scanner).get(), None);
    }
    Combat::attack(&mut scene.game, scanner, dummy);
    scene.tick();
    assert_eq!(scene.target(scanner), Some(dummy));
    assert_eq!(
        scene.game.part::<Position>(scanner).get(),
        Vec3::from((1, 0, 0))
    );
}

/// A unit with a destination is not idle, and a client's match scans for none, though an enemy
/// stands within reach of its scanner in its turn, tick 0.
#[test]
fn a_unit_with_a_destination_and_a_clients_match_scan_for_none() {
    let mut walking = Scene::new(&["enemies"], None);
    let scanner = walking.scanner((0, 0, 0));
    walking.dummy((1, 0, 0), 1);
    let away = Destination::to(Some(Vec3::from((0, 0, 9))));
    walking.game.insert_parts(scanner, away);
    walking.tick();
    assert_eq!(walking.target(scanner), None);

    let mut game = Match::builder(&ORDERS).role(Role::Client).build();
    Combat::load_weapon_stats(&mut game);
    let mut client = Scene::of(game, &["enemies"], None);
    let scanner = client.scanner((0, 0, 0));
    client.dummy((1, 0, 0), 1);
    client.tick();
    assert_eq!(client.target(scanner), None);
}
