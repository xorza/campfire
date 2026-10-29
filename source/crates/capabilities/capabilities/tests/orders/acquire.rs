//! The idle scan of `orders` (04-capabilities/control.md, An idle unit scans for a target, as
//! its type declares).

use campfire_actions::ActionProgress;
use campfire_actions::{ActionRange, ActionSlots, SlotKind, TurretMount};
use campfire_capabilities::Match;
use campfire_capabilities::Order;
use campfire_combat::Combat;
use campfire_combat::internals::{Armed, TestWeapon};
use campfire_combat::{OnDeath, Rate, Turret, TurretData, WeaponDamage};
use campfire_common::PlayerSlot;
use campfire_common::{DeclaredName, Ticks};
use campfire_declarations::Role;
use campfire_math::{Num, Vec3};
use campfire_orders::{AttackMove, Orders, OrdersDirective};
use campfire_script::Filter;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{MatchRate, Position, StableId};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{Destination, GroundPoint, Life, MoveStep, SeenBy, UnitTypeCore, Units};
use campfire_values::{Rank, Relation, RelationSet, Team, TeamSet, UnitType};
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
        Self::armed(game, combatant(100, 2, 1, 30, 10), targets, step)
    }

    /// A scene whose scanner is armed as `weapon`.
    fn armed(game: Match, weapon: Armed, targets: &[&str], step: Option<MoveStep>) -> Self {
        let mut game = game;
        let scanner = armed(&mut game, weapon, step);
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

/// A scanner whose weapon reaches 4 m and keeps off 2 m passes over an enemy at 1 m, inside its
/// minimum, for one at 3 m. Ordered at the near one, it walks straight away from it, a meter a
/// tick, toward halfway between 2 and 4 m, (1 − 3, 0, 0) = (−2, 0, 0); in tick 1 it stands at
/// (−1, 0, 0), 2 m off, at its minimum, where it stops and attacks, striking in tick 2.
#[test]
fn a_scan_passes_a_unit_inside_its_minimum_range_and_an_order_backs_away_from_it() {
    let weapon = combatant(100, 4, 1, 30, 10).keeping_off(Num::int(2));
    let mut scene = Scene::armed(orders_match(), weapon, &["enemies"], Some(meter()));
    let scanner = scene.scanner((0, 0, 0));
    let near = scene.dummy((1, 0, 0), 1);
    let far = scene.dummy((3, 0, 0), 1);
    scene.tick();
    assert_eq!(scene.target(scanner), Some(far));
    Combat::attack(&mut scene.game, scanner, near);
    scene.tick();
    assert_eq!(
        (
            scene.game.part::<Position>(scanner).get(),
            scene.game.part::<Destination>(scanner).get()
        ),
        (Vec3::from((-1, 0, 0)), Some(Vec3::from((-2, 0, 0))))
    );
    scene.tick();
    assert_eq!(
        (
            scene.game.part::<Position>(scanner).get(),
            scene.game.part::<Destination>(scanner).get()
        ),
        (Vec3::from((-1, 0, 0)), None)
    );
    scene.tick();
    assert_eq!(Stats::health(&scene.game, near), 90);
}

/// A scan reaches by any weapon, and a chase walks to the range of the weapon its unit chose
/// (04-capabilities/control.md, The scan reaches by any weapon; 04-capabilities/combat.md, The
/// chosen weapon is the attack's). A scanner of 10 within 2 m in slot 0 and of 30 within 6 m in
/// slot 1, which walks a meter a tick, takes in tick 0 an enemy at 4 m, which its second weapon
/// alone reaches, and keeps it as the second cools. Another, at x = 100, ordered at an enemy 20 m
/// off, which no weapon reaches, chooses the first and walks a meter a tick; in tick 14, at x = 86,
/// the second reaches the enemy and is its choice, and it stops there, keeping the second as it
/// cools.
#[test]
fn a_scan_reaches_by_any_weapon_and_a_chase_walks_to_the_chosen_weapons_range() {
    let mut game = orders_match();
    let scanner = Units::load_next_type(&mut game, &UnitTypeCore::default());
    Units::give_type(&mut game, scanner, Life::full(Num::int(100)));
    Units::give_type(&mut game, scanner, OnDeath::Despawn);
    Units::give_type(&mut game, scanner, meter());
    let enemies = Filter::of_match_relations(&game, RelationSet::Enemies);
    let mut weapon = |meters, amount| {
        let weapon = TestWeapon {
            rate: Rate::Period(Box::new([Ticks::new(30)])),
            damage: WeaponDamage::Amount(Box::new([Num::int(amount)])),
            ..TestWeapon::new(
                enemies,
                ActionRange::meters(Num::int(meters)),
                Ticks::new(1),
            )
        };
        (
            Combat::add_weapon(&mut game, weapon),
            SlotKind::new(0),
            Rank::new(1),
        )
    };
    let slots = ActionSlots::new([weapon(2, 10), weapon(6, 30)]);
    Units::give_type(&mut game, scanner, slots);
    Orders::load_acquire(&mut game, scanner, SCAN_MS, &["enemies"]);
    let dummy = armed(&mut game, still_target(100), None);
    let [first, second] = [0, 100].map(|x| Spawn::of(scanner, (x, 0, 0)).place(&mut game));
    let [near, far] = [4, 80].map(|x| {
        Spawn::of(dummy, (x, 0, 0))
            .team(Team::new(1))
            .place(&mut game)
    });
    let progress = |game: &Match, unit| *game.part::<ActionProgress>(unit);
    Combat::attack(&mut game, second, far);
    // After each tick: where the second stands, and its weapon.
    let mut walked = Vec::new();
    for _ in 0..=20 {
        game.step_with(&[]);
        let x = game.part::<Position>(second).get().x.to_int().unwrap();
        walked.push((x, progress(&game, second).weapon()));
    }
    assert_eq!(progress(&game, first).attack_target(), Some(near));
    let mut expected: Vec<_> = (86..=99).rev().map(|x| (x, Some(0))).collect();
    expected.extend([(86, Some(1)); 7]);
    assert_eq!(walked, expected);
}

/// A walking unit's turret takes the target its scan finds, fires on it as the unit walks on, and
/// drops it out of reach (04-capabilities/combat.md, A turret has its own target). The scanner, of
/// a turret that turns at once and fires within 180°, walks a meter a tick from the origin to (0,
/// 0, 10), scanning in ticks 0, 3 and 6; in tick 6, at z = 6, √2 m from an enemy at (1, 0, 5),
/// within its reach of 2 m, its turret takes it and its attack starts, which strikes in tick 7 as
/// the unit walks on; in tick 8, at z = 8, √10 m off, the turret drops it, and the unit stands at
/// its goal after tick 9. An attack target ends a turret target, as the unit's own comes first.
#[test]
fn a_walking_turret_fires_on_what_its_scan_finds_and_its_unit_walks_on() {
    let mut scene = Scene::new(&["enemies"], Some(meter()));
    let rate = *scene.game.resource::<MatchRate>();
    let slot_kind = DeclaredName::new("weapon").unwrap();
    let data = TurretData::new(Num::int(3600), Num::int(180), slot_kind).unwrap();
    let turret = Turret::of(&data, SlotKind::new(0), rate);
    Units::give_type(&mut scene.game, scene.scanner, turret);
    Units::give_type(&mut scene.game, scene.scanner, TurretMount);
    let scanner = scene.scanner((0, 0, 0));
    let enemy = scene.dummy((1, 0, 5), 1);
    let other = scene.dummy((50, 0, 50), 1);
    let goal = Vec3::from((0, 0, 10));
    scene
        .game
        .insert_parts(scanner, Destination::to(Some(goal)));
    // After each tick: where it stands, its turret target and the enemy's health.
    let state = |scene: &Scene| {
        let progress = scene.game.part::<ActionProgress>(scanner);
        (
            scene
                .game
                .part::<Position>(scanner)
                .get()
                .z
                .to_int()
                .unwrap(),
            progress.turret_target(),
            Stats::health(&scene.game, enemy),
        )
    };
    let mut walked = Vec::new();
    for _ in 0..10 {
        scene.tick();
        walked.push(state(&scene));
    }
    let mut expected: Vec<_> = (1..=6).map(|z| (z, None, 100)).collect();
    expected.extend([
        (7, Some(enemy), 100),
        (8, Some(enemy), 90),
        (9, None, 90),
        (10, None, 90),
    ]);
    assert_eq!(walked, expected);
    assert_eq!(scene.game.part::<Destination>(scanner).get(), None);
    assert_eq!(scene.target(scanner), None);
    // Set back to z = 4, it walks to z = 6 by its next scan, in tick 12, where its turret takes
    // the enemy again; an order at another drops it.
    *scene.game.part_mut::<Position>(scanner) = Position::from(Vec3::from((0, 0, 4)));
    scene
        .game
        .insert_parts(scanner, Destination::to(Some(goal)));
    for _ in 0..3 {
        scene.tick();
    }
    assert_eq!(
        scene.game.part::<ActionProgress>(scanner).turret_target(),
        Some(enemy)
    );
    Combat::attack(&mut scene.game, scanner, other);
    let progress = scene.game.part::<ActionProgress>(scanner);
    assert_eq!(
        (progress.turret_target(), progress.attack_target()),
        (None, Some(other))
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

/// An attack-move walks as a move, and the scan looks on its way. The scanner, player slot 0's,
/// walks a meter a tick from the origin to (0, 0, 12) and scans in ticks 0, 3 and 6, for
/// `hostiles`. In tick 3 it stands at z = 3, 2 m from a neutral at (2, 0, 3), which its scan does
/// not select, and walks on; in tick 6, at z = 6, 2 m from a hostile at (2, 0, 6), it stops and
/// takes it, and its strike lands in tick 7. Moved off to (9, 0, 6) before tick 8, the hostile
/// leaves its reach, and tick 8 drops it; in tick 9 the scanner walks on, at z = 12 after tick 14,
/// and tick 15 ends its attack-move; another order drops one under way (04-capabilities/control.md,
/// An army attack-moves).
#[test]
fn an_attack_move_takes_what_its_scan_finds_on_the_way_and_walks_on() {
    let mut scene = Scene::new(&["hostiles"], Some(meter()));
    Units::set_relation(
        &mut scene.game,
        Team::new(0),
        Team::new(2),
        Relation::Neutral,
    );
    let scanner = Spawn::of(scene.scanner, (0, 0, 0))
        .team(Team::new(0))
        .owner(PlayerSlot::new(0))
        .place(&mut scene.game);
    scene.dummy((2, 0, 3), 2);
    let hostile = scene.dummy((2, 0, 6), 1);
    let goal = Vec3::from((0, 0, 12));
    let to = GroundPoint {
        x: Num::ZERO,
        z: Num::int(12),
    };
    scene.game.step_orders(
        PlayerSlot::new(0),
        &[Order::one(scanner, OrdersDirective::AttackMove { to })],
    );
    // After each tick: where it stands, where it walks to, its target, and its goal.
    let state = |scene: &Scene| {
        (
            scene.game.part::<Position>(scanner).get().z,
            scene.game.part::<Destination>(scanner).get(),
            scene.target(scanner),
            scene.game.part::<AttackMove>(scanner).goal(),
        )
    };
    let z = |meters: i64| Num::int(meters);
    assert_eq!(state(&scene), (z(1), Some(goal), None, Some(goal)));
    for _ in 1..7 {
        scene.tick();
    }
    assert_eq!(state(&scene), (z(6), None, Some(hostile), Some(goal)));
    scene.tick();
    assert_eq!(Stats::health(&scene.game, hostile), 90);
    *scene.game.part_mut::<Position>(hostile) = Position::from(Vec3::from((9, 0, 6)));
    scene.tick();
    assert_eq!(state(&scene), (z(6), None, None, Some(goal)));
    scene.tick();
    assert_eq!(state(&scene), (z(7), Some(goal), None, Some(goal)));
    for _ in 10..15 {
        scene.tick();
    }
    assert_eq!(state(&scene), (z(12), None, None, Some(goal)));
    scene.tick();
    assert_eq!(state(&scene), (z(12), None, None, None));
    // An attack-move back to the origin gives the goal again; a stop in the next tick drops it.
    let to = GroundPoint {
        x: Num::ZERO,
        z: Num::ZERO,
    };
    scene.game.step_orders(
        PlayerSlot::new(0),
        &[Order::one(scanner, OrdersDirective::AttackMove { to })],
    );
    assert_eq!(state(&scene).3, Some(Vec3::ZERO));
    scene.game.step_orders(
        PlayerSlot::new(0),
        &[Order::one(scanner, OrdersDirective::Stop)],
    );
    assert_eq!(state(&scene), (z(11), None, None, None));
}
