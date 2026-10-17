use campfire_actions::ShotDamage;
use campfire_combat::{SCATTER_STREAM, SplashData};
use campfire_math::SinCos;
use campfire_units::{CellSplit, Ground, HeightGrid};
use campfire_values::{ActionId, DamageKind, Rank, RelationSet};

use super::*;

/// A shell of `speed` m/s and `width`, on a curve whose inner points rise 4 m above its ends at a
/// quarter and three quarters of the way, its end following its target by `follow` m/s, its life
/// `life_ms`.
fn shell(speed: i64, width: Num, follow: i64, life_ms: Option<u32>) -> ProjectileData {
    ProjectileData {
        width,
        curve: Some(CurveData {
            first: [Num::QUARTER, Num::int(4)],
            second: [Num::QUARTER * 3, Num::int(4)],
            follow: Num::int(follow),
            life_ms,
        }),
        ..ProjectileData::flying(Num::int(speed))
    }
}

/// The unit types of a shelling: shells of `data`, a shooter of them, and targets of a body of
/// 1 m, of no height, of `height` and 4 m tall.
#[derive(Debug, Clone, Copy)]
struct Shelling {
    shell: UnitType,
    shooter: UnitType,
    flat: UnitType,
    tall: UnitType,
    tower: UnitType,
}

impl Shelling {
    fn place(game: &mut Match, data: &ProjectileData, height: Num) -> Self {
        let shell = Units::load_type(game, TypeScope::Mode, "shell", &UnitTypeCore::default());
        Projectiles::load_type(game, shell, data);
        let shooter = shooter(shell).load(game);
        let [flat, tall, tower] = [(); 3].map(|()| target().load(game));
        let round = BodyForm::circle(Num::ONE).unwrap();
        Units::give_type(game, flat, round);
        Units::give_type(game, tall, round.raised(height).unwrap());
        Units::give_type(game, tower, round.raised(Num::int(4)).unwrap());
        Self {
            shell,
            shooter,
            flat,
            tall,
            tower,
        }
    }
}

/// The flight of the one projectile of `game`.
fn flight(game: &mut Match) -> Flight {
    let projectile = game.query::<&Projectile>().single(game).unwrap();
    projectile.flight()
}

/// The state of the one projectile of `game`.
fn state(game: &mut Match) -> FlightState {
    *game.query::<&FlightState>().single(game).unwrap()
}

/// Moves the unit `id` of `game` to `pos`.
fn put(game: &mut Match, id: StableId, pos: Vec3) {
    let entity = game.unit_entity(id);
    *game.get_mut::<Position>(entity).unwrap() = Position::from(pos);
}

#[test]
fn a_shell_stands_at_a_point_of_its_curve_each_tick_and_hits_its_target_at_the_last() {
    // 120 m/s, 4 m a tick. From the shooter at the origin to the target's center at (8, 0, 0),
    // its inner points are (2, 4, 0) and (6, 4, 0); its length, by the split rule, about 10.54,
    // so its points are ⌈10.54 ÷ 4⌉ = 3: the origin, the curve at t = ½, (4, 3, 0), and the end.
    let mut game = volley_match();
    let volley = Shelling::place(&mut game, &shell(120, Num::ZERO, 0, None), Num::ZERO);
    let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, volley.flat, (8, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, shooter, target);
    // It fires in tick 2 and spawns at its first point; in tick 3 it stands there, and in tick 4
    // at the next, 5 m flown.
    for _ in 0..4 {
        game.step();
    }
    let curve = Flight::Curve {
        from: Vec3::ZERO,
        to: Vec3::from((8, 0, 0)),
        aimed: Some(target),
        points: 3,
    };
    assert_eq!(Projectiles::projectiles(&game), [Vec3::ZERO]);
    game.step();
    let end = Vec3::from((8, 0, 0));
    let flown = FlightState::Curve {
        flown: Num::int(5),
        steps: 2,
        end,
    };
    assert_eq!(flight(&mut game), curve);
    assert_eq!(state(&mut game), flown);
    assert_eq!(Projectiles::projectiles(&game), [Vec3::from((4, 3, 0))]);
    // In tick 5, at its last point, its sphere meets the target's body, which takes the hit.
    game.step();
    assert_eq!(Projectiles::projectiles(&game), []);
    assert_eq!(Stats::health(&game, target), 70);
}

#[test]
fn a_shells_end_follows_its_target_by_follow_a_tick_past_a_tenth_of_a_square_meter() {
    // 30 m/s, 1 m a tick. The target moves 2 m along z as the shell launches: in tick 3 its end
    // moves 1 m toward it, to (8, 0, 1), and in tick 4 to (8, 0, 2), where its curve's inner
    // points lie 0.5 and 1.5 m along z, so its middle is (4, 3, (1.5 + 4.5 + 2) ÷ 8 = 1); in tick 5
    // it hits the target there.
    let run = |follow: i64, moved: Num| {
        let mut game = volley_match();
        let volley = Shelling::place(&mut game, &shell(120, Num::ZERO, follow, None), Num::ZERO);
        let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
        let target = Mode::spawn(&mut game, volley.flat, (8, 0, 0), Team::new(1), ());
        Combat::attack(&mut game, shooter, target);
        for _ in 0..3 {
            game.step();
        }
        put(&mut game, target, Vec3::new(Num::int(8), Num::ZERO, moved));
        (game, target)
    };
    let end = |game: &mut Match| match state(game) {
        FlightState::Curve { end, .. } => end,
        FlightState::Flown { .. } => panic!("a curve's state"),
    };
    let (mut game, target) = run(30, Num::int(2));
    game.step();
    assert_eq!(end(&mut game), Vec3::from((8, 0, 1)));
    game.step();
    assert_eq!(end(&mut game), Vec3::from((8, 0, 2)));
    assert_eq!(Projectiles::projectiles(&game), [Vec3::from((4, 3, 1))]);
    game.step();
    assert_eq!(Stats::health(&game, target), 70);
    // With no `follow`, its end stays: in tick 5 it stands 1 m from the target's edge, and meets
    // nothing; in tick 6, past its last point, it ends there.
    let (mut game, target) = run(0, Num::int(2));
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(Projectiles::projectiles(&game), [Vec3::from((8, 0, 0))]);
    game.step();
    assert_eq!(Projectiles::projectiles(&game), []);
    assert_eq!(Stats::health(&game, target), 100);
    // A target 0.3 m away, 0.09 m², is within a tenth of a square meter, and the end stays; one
    // 0.35 m away, 0.1225 m², is past it, and the end reaches it, within 1 m.
    let hundredths = |hundredths: i64| Num::from_int(hundredths).unwrap() / 100;
    let (mut game, _) = run(30, hundredths(30));
    game.step();
    assert_eq!(end(&mut game), Vec3::from((8, 0, 0)));
    let (mut game, _) = run(30, hundredths(35));
    game.step();
    let near = Vec3::new(Num::int(8), Num::ZERO, hundredths(35));
    assert_eq!(end(&mut game), near);
}

#[test]
fn a_shells_sphere_meets_the_lowest_id_it_selects_on_its_way_and_never_its_source() {
    // A shell 2 m wide hits hostiles and allies, by a filter each, from a shooter of a body 1 m wide and 2 m tall, its center at
    // (0, 1, 0), to the center of a target 2 m tall at (8, 1, 0): its inner points rise to 5 m,
    // and its middle is (4, (1 + 15 + 15 + 1) ÷ 8 = 4, 0).
    let filters = |names: &[&str]| {
        let filters = names.iter().map(|name| FilterData::parse(name).unwrap());
        ProjectileData {
            hits: ProjectileHits::Units(filters.collect()),
            ..shell(120, Num::int(2), 0, None)
        }
    };
    let both = filters(&["hostiles", "allies"]);
    let fixture = |game: &mut Match, data: &ProjectileData| {
        let volley = Shelling::place(game, data, Num::int(2));
        let tall = BodyForm::circle(Num::ONE)
            .unwrap()
            .raised(Num::int(2))
            .unwrap();
        Units::give_type(game, volley.shooter, tall);
        volley
    };
    // Two enemies 4 m tall beside its middle, 1.5 m either side along z: its sphere, 1 m wide,
    // lies 0.5 m from each, on the plane of their tops, and meets both. The lower id takes it.
    let mut game = volley_match();
    let volley = fixture(&mut game, &both);
    let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, volley.tall, (8, 0, 0), Team::new(1), ());
    let half = Num::HALF * 3;
    let [first, second] = [-half, half].map(|z| {
        Mode::spawn(
            &mut game,
            volley.tower,
            (Num::int(4), Num::ZERO, z),
            Team::new(1),
            (),
        )
    });
    Combat::attack(&mut game, shooter, target);
    for _ in 0..5 {
        game.step();
    }
    let healths = [first, second, target, shooter].map(|id| Stats::health(&game, id));
    assert_eq!(healths, [70, 100, 100, 100]);
    assert_eq!(Projectiles::projectiles(&game), []);
    // In tick 3, at its first point, the shooter's own center, it meets its source's body, which
    // it passes, and a friend's 1.5 m beside it, which its filter of allies hits; of hostiles
    // alone, it passes the friend and hits its target at its last point, in tick 5.
    let friendly_fire = |data: &ProjectileData| {
        let mut game = volley_match();
        let volley = fixture(&mut game, data);
        let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
        let target = Mode::spawn(&mut game, volley.tall, (8, 0, 0), Team::new(1), ());
        let friend = Mode::spawn(
            &mut game,
            volley.tall,
            (half, Num::ZERO, Num::ZERO),
            Team::new(0),
            (),
        );
        Combat::attack(&mut game, shooter, target);
        for _ in 0..6 {
            game.step();
        }
        [friend, target, shooter].map(|id| Stats::health(&game, id))
    };
    assert_eq!(friendly_fire(&both), [70, 100, 100]);
    assert_eq!(friendly_fire(&filters(&["hostiles"])), [100, 70, 100]);
}

#[test]
fn a_shell_rises_over_the_highest_ground_on_its_way_and_ends_as_its_life_passes() {
    // A ground of samples 10 m apart, 2 m high but 5 m at (0, 10): the line from the origin to
    // (8, 0) crosses its one cell, whose highest is 5 m, while the ground under it is 2 m. The
    // shell's ends lie on the ground, 2 m up; its inner points rise to 5 + 4 = 9 m, so its middle
    // is (2 + 27 + 27 + 2) ÷ 8 = 7.25 m up, 5.25 m above the ground. At 180 m/s, 6 m a tick, its
    // length, between its chord 8 and twice its middle's rise, 2 · √(4² + 5.25²) ≈ 13.2, takes 3
    // points.
    let mut game = volley_match();
    let grid =
        HeightGrid::new([Num::ZERO; 2], Num::int(10), Num::ONE, 2, vec![2, 2, 5, 2]).unwrap();
    let ground = Ground::new(grid, CellSplit::ZeroTwo, None).unwrap();
    let map = MapBook {
        ground,
        ..game.resource::<MapBook>().clone()
    };
    game.insert_resource(map);
    let volley = Shelling::place(&mut game, &shell(180, Num::ZERO, 0, None), Num::ZERO);
    let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, volley.flat, (8, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, shooter, target);
    for _ in 0..4 {
        game.step();
    }
    let Flight::Curve {
        from, to, points, ..
    } = flight(&mut game)
    else {
        panic!("a curve");
    };
    assert_eq!([from, to], [Vec3::from((0, 2, 0)), Vec3::from((8, 2, 0))]);
    assert_eq!(points, 3);
    let middle = Vec3::new(Num::int(4), Num::int(21) / 4, Num::ZERO);
    game.step();
    assert_eq!(Projectiles::projectiles(&game), [middle]);
    game.step();
    assert_eq!(Stats::health(&game, target), 70);

    // A life of 50 ms is 1.5 ticks at 30 Hz, 2: in tick 3 it stands at its first point, and in
    // tick 4, the second since its launch, it ends there.
    let mut game = volley_match();
    let volley = Shelling::place(&mut game, &shell(120, Num::ZERO, 0, Some(50)), Num::ZERO);
    let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, volley.flat, (8, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, shooter, target);
    for _ in 0..4 {
        game.step();
    }
    assert_eq!(Projectiles::projectiles(&game), [Vec3::ZERO]);
    game.step();
    assert_eq!(Projectiles::projectiles(&game), []);
    assert_eq!(Stats::health(&game, target), 100);
}

#[test]
fn a_restored_curve_holds_its_points_and_its_state_within_them() {
    let mut game = volley_match();
    let volley = Shelling::place(&mut game, &shell(120, Num::ZERO, 0, None), Num::ZERO);
    let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, volley.flat, (8, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, shooter, target);
    for _ in 0..4 {
        game.step();
    }
    let (entity, projectile, &state) = game
        .query::<(Entity, &Projectile, &FlightState)>()
        .single(&game)
        .unwrap();
    let projectile = projectile.clone();
    // A curve of 1 to `2²⁶ + 1` points; a state at most at its last, its end within the world's
    // bound; a state of a line, or a homing flight of a curve's type, refused.
    let Flight::Curve {
        from, to, aimed, ..
    } = projectile.flight()
    else {
        panic!("a curve");
    };
    let of = |points| Flight::Curve {
        from,
        to,
        aimed,
        points,
    };
    let payload = Payload::Attack(ShotDamage {
        action: ActionId::new(0),
        rank: Rank::FIRST,
        amount: Num::int(30),
        kind: DamageKind::new(0),
        roll: Num::ZERO,
    });
    let made = [0, 1, (1 << 26) + 1, (1 << 26) + 2]
        .map(|points| Projectile::new(shooter, of(points), payload).is_some());
    assert_eq!(made, [false, true, true, false]);
    let curve = |steps, end| FlightState::Curve {
        flown: Num::ZERO,
        steps,
        end,
    };
    let past = Vec3::new(Position::BOUND + Num::ONE, Num::ZERO, Num::ZERO);
    let checks = [
        state,
        curve(3, to),
        curve(4, to),
        curve(1, past),
        FlightState::START,
    ]
    .map(|state| state.check(&game, entity));
    assert_eq!(checks, [true, true, false, false, false]);
    let homing = Projectile::new(shooter, Flight::Homing { target }, payload);
    assert!(!homing.unwrap().check(&game, entity));
    assert!(projectile.check(&game, entity));
    let negative = FlightState::Curve {
        flown: -Num::EPSILON,
        steps: 0,
        end: to,
    };
    assert!(Binary::decode::<FlightState>(&Binary::encode(&negative)).is_err());
}

/// A match of a shelling, a shooter of the weapon `arm` makes at the origin of team 0 attacking a
/// target of a body of 1 m at (8, 0, 0) of team 1: the match, the shooter and the target.
#[derive(Debug)]
struct Shot {
    game: Match,
    shooter: StableId,
    target: StableId,
}

impl Shot {
    fn new(arm: impl FnOnce(Armed) -> Armed, place: impl FnOnce(&mut Match, Shelling)) -> Self {
        let mut game = volley_match();
        let mut volley = Shelling::place(&mut game, &shell(120, Num::ZERO, 0, None), Num::ZERO);
        volley.shooter = arm(shooter(volley.shell)).load(&mut game);
        let shooter = Mode::spawn(&mut game, volley.shooter, (0, 0, 0), Team::new(0), ());
        let target = Mode::spawn(&mut game, volley.flat, (8, 0, 0), Team::new(1), ());
        place(&mut game, volley);
        Combat::attack(&mut game, shooter, target);
        Self {
            game,
            shooter,
            target,
        }
    }

    /// Steps its match from its start through tick `tick`.
    fn through(&mut self, tick: u64) {
        for _ in 0..=tick {
            self.game.step();
        }
    }
}

#[test]
fn a_shells_splash_damages_the_units_of_its_relations_within_its_radius_once_each() {
    // A splash of 2 m at the enemies: the shell ends on the target's center, (8, 0, 0), in tick
    // 5. An enemy whose body's edge lies 1.5 m from it takes the damage, one 2 m away does not,
    // nor an ally within it; the target takes it once, its hit dealing nothing of its own.
    let splash = SplashData {
        radius: Num::int(2),
        affects: RelationSet::Enemies,
        spares_source: true,
    };
    let mut near = Vec::new();
    let mut shot = Shot::new(
        |armed| armed.splashing(splash),
        |game, volley| {
            let half = Num::HALF;
            let at = |x: Num, z: Num| Vec3::new(x, Num::ZERO, z);
            for (pos, team) in [
                (at(Num::int(10) + half, Num::ZERO), 1),
                (at(Num::int(11), Num::ZERO), 1),
                (at(Num::int(8), -Num::int(2) - half), 0),
            ] {
                near.push(Mode::spawn(game, volley.flat, pos, Team::new(team), ()));
            }
        },
    );
    shot.through(5);
    let [inside, at_edge, ally] = near[..] else {
        panic!("three units near");
    };
    let healths = [shot.target, inside, at_edge, ally].map(|id| Stats::health(&shot.game, id));
    assert_eq!(healths, [70, 70, 100, 100]);

    // At the allies, 9 m wide, sparing no source: the shooter, a point 8 m away, takes it, and
    // the target, an enemy, does not; sparing its source, the shooter takes nothing.
    let run = |spares_source| {
        let splash = SplashData {
            radius: Num::int(9),
            affects: RelationSet::Allies,
            spares_source,
        };
        let mut shot = Shot::new(|armed| armed.splashing(splash), |_, _| {});
        shot.through(5);
        [shot.shooter, shot.target].map(|id| Stats::health(&shot.game, id))
    };
    assert_eq!(run(false), [70, 100]);
    assert_eq!(run(true), [100, 100]);
}

#[test]
fn a_shell_scatters_to_the_point_its_two_draws_give_and_follows_no_unit() {
    // A scatter of 3 m: in tick 2 its attack draws twice on its own stream, a distance of 3 m
    // times the first and a turn times the second, from the target's ground point, (8, 0, 0).
    let mut shot = Shot::new(|armed| armed.scattering(Num::int(3)), |_, _| {});
    shot.through(3);
    let mut draws = Rng::new(
        &SegmentSeed::ZERO,
        SCATTER_STREAM,
        shot.shooter.get(),
        Tick::new(2),
    );
    let [distance, turn] = [draws.fraction(), draws.fraction()];
    let distance = Num::int(3) * distance;
    let SinCos { sin, cos } = SinCos::of_degrees(Num::int(360) * turn);
    let to = Vec3::new(Num::int(8) + distance * cos, Num::ZERO, -(distance * sin));
    let Flight::Curve {
        to: aimed_at,
        aimed,
        ..
    } = flight(&mut shot.game)
    else {
        panic!("a curve");
    };
    assert_eq!((aimed_at, aimed), (to, None));
    assert_ne!(to, Vec3::from((8, 0, 0)));
    // A scatter of 0 aims at the target itself.
    let mut shot = Shot::new(|armed| armed.scattering(Num::ZERO), |_, _| {});
    shot.through(3);
    let Flight::Curve { aimed, .. } = flight(&mut shot.game) else {
        panic!("a curve");
    };
    assert_eq!(aimed, Some(shot.target));
}

#[test]
fn a_target_past_a_weapons_pitch_bound_is_out_of_its_reach() {
    // From −10° to 30°: the target, its center 6 m up and 8 m along, at 36.9°, is past it, and
    // the shooter never attacks; 2 m down, −14°, below it; 4 m up, 26.6°, within it.
    let run = |up: i32| {
        let pitched = |armed: Armed| armed.pitched(Num::int(-10), Num::int(30));
        let mut shot = Shot::new(pitched, |_, _| {});
        put(&mut shot.game, shot.target, Vec3::from((8, up, 0)));
        shot.through(8);
        Stats::health(&shot.game, shot.target)
    };
    assert_eq!([run(6), run(-2), run(4)], [100, 100, 70]);
}
