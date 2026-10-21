use campfire_capabilities::Match;
use campfire_combat::internals::Armed;
use campfire_combat::{Combat, OnDeath};
use campfire_math::{Num, Vec3};
use campfire_projectiles::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Position, SimWorld, StableId};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{BodyForm, Dead, UnitTypeCore, Units};
use campfire_values::{Team, TypeScope, UnitType};

use crate::projectiles::{target, volley_match};

/// A missile of 30 m/s, 1 m a tick at the tests' 30 ticks a second, steered as `steer` says.
fn missile(steer: SteerData) -> ProjectileData {
    ProjectileData {
        steer: Some(steer),
        ..ProjectileData::flying(Num::int(30))
    }
}

/// A steer of `turn_rate` degrees a second that flies for 10 s and leaves at its missile's speed,
/// straight at once, with no lock and no cruise, tracking its target.
const fn steer(turn_rate: i64) -> SteerDataBuilder {
    SteerData::builder(Num::int(turn_rate), 10_000)
}

/// The unit types of a steering: a shooter of missiles of `data`, and targets of a body of 1 m,
/// of no height and 2 m tall.
#[derive(Debug, Clone, Copy)]
struct Steering {
    shooter: UnitType,
    round: UnitType,
    tall: UnitType,
}

impl Steering {
    fn place(game: &mut Match, data: &ProjectileData) -> Self {
        let missile = Units::load_type(game, TypeScope::Mode, "missile", &UnitTypeCore::default());
        Projectiles::load_type(game, missile, data);
        // 30 damage within 20 m, fired 2 ticks after the start of an attack every 300 ticks, so
        // one missile flies at a time.
        let shooter = Armed::melee(Num::int(100), Num::int(20), 2, 300, Num::int(30))
            .ranged(missile)
            .on_death(OnDeath::Stay)
            .load(game);
        let [round, tall] = [(); 2].map(|()| target().load(game));
        let circle = BodyForm::circle(Num::ONE).unwrap();
        Units::give_type(game, round, circle);
        Units::give_type(game, tall, circle.raised(Num::int(2)).unwrap());
        Self {
            shooter,
            round,
            tall,
        }
    }

    /// A shooter at the origin attacking a round target of team 1 at `at`, and the target,
    /// once its missile flies, in tick 2.
    fn launch(self, game: &mut Match, at: impl Into<Vec3>) -> StableId {
        self.launch_at(game, self.round, at)
    }

    /// A shooter at the origin attacking a target of `unit_type` and team 1 at `at`, and the
    /// target, once its missile flies, in tick 2.
    fn launch_at(self, game: &mut Match, unit_type: UnitType, at: impl Into<Vec3>) -> StableId {
        let shooter = Spawn::of(self.shooter, (0, 0, 0)).place(game);
        let target = Spawn::of(unit_type, at).team(Team::new(1)).place(game);
        Combat::attack(game, shooter, target);
        let mut ticks = 0;
        while Projectiles::projectiles(game).is_empty() {
            game.step();
            ticks += 1;
            assert!(ticks < 20, "no missile launched");
        }
        target
    }
}

/// The steered state of the one projectile of `game`: the meters it flew, the ticks it flew, its
/// speed a tick, its heading, and whether it lost its target.
fn state(game: &mut Match) -> (Num, u32, Num, Vec3, bool) {
    let FlightState::Steered {
        flown,
        steps,
        speed,
        heading,
        lost,
    } = *game.query::<&FlightState>().single(game).unwrap()
    else {
        panic!("a steered flight's state");
    };
    (flown, steps, speed, heading.get(), lost)
}

/// Where the one projectile of `game` stands, none when it ended.
fn standing(game: &Match) -> Option<Vec3> {
    let projectiles = Projectiles::projectiles(game);
    assert!(projectiles.len() <= 1, "one projectile");
    projectiles.first().copied()
}

/// Whether `got` lies within `bits` of 2⁻²⁴ of `want`'s decimals on each axis.
fn near(got: Vec3, want: [&str; 3], bits: i64) -> bool {
    [got.x, got.y, got.z]
        .into_iter()
        .zip(want.map(|text| text.parse::<Num>().unwrap()))
        .all(|(got, want)| (got - want).to_bits().abs() <= bits)
}

/// Moves the unit `id` of `game` to `position`.
fn put(game: &mut Match, id: StableId, position: Vec3) {
    let entity = game.unit_entity(id);
    *game.get_mut::<Position>(entity).unwrap() = Position::from(position);
}

#[test]
fn a_steered_missile_speeds_up_from_its_launch_and_strikes_its_target() {
    // It leaves at 15 m/s, half a meter a tick, and gains 225 m/s², 225 / 30² = ¼ m a tick each
    // tick, up to 1 m a tick: its steps are ¾ and then 1 m, along x toward the target at
    // (10, 0, 0), standing at ¾, 1¾, …, 9¾, where its point lies within the target's body of
    // 1 m, which takes its 30.
    let data = missile(
        steer(1350)
            .launch_speed(Num::int(15))
            .acceleration(Num::int(225))
            .build()
            .unwrap(),
    );
    let mut game = volley_match();
    let target = Steering::place(&mut game, &data).launch(&mut game, (10, 0, 0));
    let along = |x: Num| Vec3::new(x, Num::ZERO, Num::ZERO);
    let flying = |flown: Num, steps, speed| (flown, steps, speed, along(Num::ONE), false);
    assert_eq!(standing(&game), Some(Vec3::ZERO));
    assert_eq!(state(&mut game), flying(Num::ZERO, 0, Num::HALF));
    let three_quarters = Num::QUARTER * 3;
    game.step();
    assert_eq!(standing(&game), Some(along(three_quarters)));
    assert_eq!(state(&mut game), flying(three_quarters, 1, three_quarters));
    game.step();
    let seven_quarters = three_quarters + Num::ONE;
    assert_eq!(standing(&game), Some(along(seven_quarters)));
    assert_eq!(state(&mut game), flying(seven_quarters, 2, Num::ONE));
    for _ in 0..7 {
        game.step();
    }
    assert_eq!(standing(&game), Some(along(seven_quarters + Num::int(7))));
    assert_eq!(Stats::health(&game, target), 100);
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 70);
}

#[test]
fn a_steered_missile_turns_by_at_most_its_bound_after_its_straight_run() {
    // Launched along x at a target at (10, 0, 0), which moves to (1, 0, 10): in its straight run
    // of 1 m its first step keeps x, to (1, 0, 0). Its next turns a quarter turn, to z: whole at
    // 90° a tick, 2,700 °/s, to (1, 0, 1), then on to (1, 0, 9), where its point meets the
    // target's body; at 45° a tick only to (cos 45°, 0, sin 45°), (1 + √½, 0, √½).
    let flight = |turn_rate| {
        let data = missile(steer(turn_rate).straight(Num::ONE).build().unwrap());
        let mut game = volley_match();
        let target = Steering::place(&mut game, &data).launch(&mut game, (10, 0, 0));
        put(&mut game, target, Vec3::from((1, 0, 10)));
        game.step();
        assert_eq!(standing(&game), Some(Vec3::from((1, 0, 0))));
        game.step();
        (game, target)
    };
    let (mut game, target) = flight(2700);
    assert_eq!(standing(&game), Some(Vec3::from((1, 0, 1))));
    for _ in 0..7 {
        game.step();
    }
    assert_eq!(standing(&game), Some(Vec3::from((1, 0, 8))));
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 70);
    let (game, _) = flight(1350);
    let half = "0.7071067812";
    assert!(near(
        standing(&game).unwrap(),
        ["1.7071067812", "0", half],
        4
    ));
}

#[test]
fn a_lock_flies_straight_onto_its_target_where_a_steer_of_no_turn_flies_by() {
    // Launched along x at a target at (10, 0, 0), which moves to (0, 0, 3), with no turn: within
    // a lock of 10 m it steps straight at the target's center, to (0, 0, 1) and (0, 0, 2), where
    // its point meets the target's body; with no lock it flies on along x, and ends as its fuel
    // of 100 ms, 3 ticks, runs out, standing at (3, 0, 0), striking nothing.
    let flight = |lock: i64| {
        let data = missile(
            SteerData::builder(Num::ZERO, 100)
                .lock(Num::int(lock))
                .build()
                .unwrap(),
        );
        let mut game = volley_match();
        let target = Steering::place(&mut game, &data).launch(&mut game, (10, 0, 0));
        put(&mut game, target, Vec3::from((0, 0, 3)));
        (game, target)
    };
    let (mut game, target) = flight(10);
    game.step();
    assert_eq!(standing(&game), Some(Vec3::from((0, 0, 1))));
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 70);
    let (mut game, target) = flight(0);
    for x in 1..=3 {
        game.step();
        assert_eq!(standing(&game), Some(Vec3::from((x, 0, 0))));
    }
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 100);
}

#[test]
fn a_steered_missile_strikes_a_unit_on_its_way_and_ends_as_its_target_dies() {
    // Along x at a target at (10, 0, 0): an enemy of a body of 1 m at (5, 0, 0) stands on its way,
    // and its point meets that enemy's body at 4 m, in its fourth step, which takes the 30.
    let data = missile(steer(1350).build().unwrap());
    let mut game = volley_match();
    let steering = Steering::place(&mut game, &data);
    let between = Spawn::of(steering.round, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let target = steering.launch(&mut game, (10, 0, 0));
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(standing(&game), Some(Vec3::from((3, 0, 0))));
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(
        [between, target].map(|id| Stats::health(&game, id)),
        [70, 100]
    );

    // A target that dies ends it in its next step, striking nothing.
    let mut game = volley_match();
    let target = Steering::place(&mut game, &data).launch(&mut game, (10, 0, 0));
    game.step();
    let entity = game.unit_entity(target);
    game.entity_mut(entity).insert(Dead);
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 100);
}

#[test]
fn a_cruising_missile_holds_its_height_until_its_dive_and_one_ends_on_the_ground() {
    // Turning any way, 5,400 °/s or half a turn a tick, at a target 2 m tall at (10, 0, 0),
    // cruising 3 m up until within 4 m of it on the ground: it steers at (10, 3, 0), its step
    // (10, 3, 0) / √109 from the origin, to (0.957826, 0.287348, 0), and stays up; from x = 6 on,
    // within its dive, it steers at the target's center, (10, 1, 0), and strikes it.
    let data = missile(
        steer(5400)
            .cruise(CruiseData::new(Num::int(3), Num::int(4)))
            .build()
            .unwrap(),
    );
    let mut game = volley_match();
    let steering = Steering::place(&mut game, &data);
    let target = steering.launch_at(&mut game, steering.tall, (10, 0, 0));
    game.step();
    assert!(near(
        standing(&game).unwrap(),
        ["0.9578262852", "0.2873478856", "0"],
        4
    ));
    while standing(&game).is_some_and(|at| at.x < Num::int(6)) {
        assert!(standing(&game).unwrap().y > Num::ZERO);
        game.step();
    }
    while standing(&game).is_some() {
        game.step();
    }
    assert_eq!(Stats::health(&game, target), 70);

    // At a target 2 m below the ground at (8, 0, 0), turning any way: its first step, toward the
    // target's center, (8, −2, 0) / √68, passes under the ground, where it ends, striking
    // nothing.
    let data = missile(steer(5400).build().unwrap());
    let mut game = volley_match();
    let target = Steering::place(&mut game, &data).launch(&mut game, (8, -2, 0));
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 100);
}

#[test]
fn a_flight_to_a_point_locks_on_within_its_points_lock_its_units_lock_by_default() {
    // Tracking no target, turning any way, at a target at (4, 0, 0), cruising 3 m up with no dive:
    // within a point's lock of 10 m, its own or its lock's, it steps straight at the point, to
    // (1, 0, 0); within a point's lock of 0 it steers at (4, 3, 0), its step (4, 3, 0) / 5, to
    // (0.8, 0.6, 0), however wide its lock at a unit.
    let first = |lock: SteerDataBuilder| {
        let cruise = CruiseData::new(Num::int(3), Num::ZERO);
        let data = missile(lock.cruise(cruise).tracks(false).build().unwrap());
        let mut game = volley_match();
        Steering::place(&mut game, &data).launch(&mut game, (4, 0, 0));
        game.step();
        standing(&game).unwrap()
    };
    let ten = Num::int(10);
    assert_eq!(first(steer(5400).point_lock(ten)), Vec3::from((1, 0, 0)));
    assert_eq!(first(steer(5400).lock(ten)), Vec3::from((1, 0, 0)));
    let unlocked = first(steer(5400).lock(ten).point_lock(Num::ZERO));
    assert!(near(unlocked, ["0.8", "0.6", "0"], 4), "{unlocked:?}");
}

#[test]
fn a_missile_that_tracks_no_target_flies_to_where_it_stood_and_ends_there() {
    // Tracking no target, at a target at (4, 0, 0) that moves to (0, 0, 4): it flies on to
    // (4, 0, 0), a meter a tick, and ends there, its target struck by nothing.
    let data = missile(steer(1350).tracks(false).build().unwrap());
    let mut game = volley_match();
    let target = Steering::place(&mut game, &data).launch(&mut game, (4, 0, 0));
    put(&mut game, target, Vec3::from((0, 0, 4)));
    for x in 1..4 {
        game.step();
        assert_eq!(standing(&game), Some(Vec3::from((x, 0, 0))));
    }
    game.step();
    assert_eq!(standing(&game), None);
    assert_eq!(Stats::health(&game, target), 100);
}
