use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use campfire_codec::{Binary, Toml};
use campfire_common::{SegmentSeed, Tick};
use campfire_math::RngSource;
use campfire_sim::{SimComponent, SimWorld, SnapshotError};

use super::*;
use crate::combat::ROLL_STREAM;
use crate::combat::combat_world::test_world::CombatWorldTest;
use crate::combat::deaths::Deaths;
use crate::combat::internals::Armed;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attack::RecentAttack;
use crate::combat::recent_attackers::RecentAttackers;
use crate::geometry::body_box::BodyBox;
use crate::geometry::grid::Grid;
use crate::navigation::navigation_effect::NavigationEffect;
use crate::projectiles::projectile_data::{ProjectileData, ProjectileHits};
use crate::projectiles::projectiles_world::test_world::{ProjectilesWorldTest, Volley};
use crate::projectiles::struck_units::Struck;
use crate::release::r#match::Match;
use crate::release::release_world::test_world::ReleaseWorldTest;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::life::Life;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::DashTo;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::units::units_world::test_world::UnitsWorldTest;
use crate::values::filter_data::FilterData;
use crate::vision::seen_by::SeenBy;
use crate::vision::sight::Sight;
use crate::vision::vision_world::VisionWorld;

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap()
}

/// A projectile type of 15 m/s, half a meter a tick, that hits enemies; homing, or along a line
/// of `width` for `range` that ends at its first hit when `stop_on_hit`.
fn projectile(homing: bool, width: Num, range: Option<Num>, stop_on_hit: bool) -> ProjectileData {
    ProjectileData {
        width,
        range,
        homing,
        stop_on_hit,
        ..ProjectileData::flying(Num::int(15))
    }
}

/// 30 damage within 8 m, fired 2 ticks after the start of an attack every 10 ticks, its
/// projectiles of the homing type `bolt`.
fn shooter(bolt: UnitType) -> Armed {
    Armed::melee(Num::int(100), Num::int(8), 2, 10, Num::int(30))
        .ranged(bolt)
        .on_death(OnDeath::Stay)
}

/// 100 health; never attacks; stays when it dies.
fn target() -> Armed {
    Armed::unarmed(Num::int(100)).on_death(OnDeath::Stay)
}

/// A match with combat and projectiles, and the projectiles' types.
fn volley_match() -> Match {
    volley_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Projectiles,
    ])
}

/// A match of `declared`, with combat and projectiles among them, and the projectiles' types.
fn volley_match_of(declared: &[Capability]) -> Match {
    let budgets = ScriptBudgets::new(ScriptLimits::ROOMY, 1);
    let mut game = Match::server(declared, budgets);
    let types = [
        ("bolt", projectile(true, Num::ZERO, None, false)),
        (
            "lance",
            projectile(false, Num::ONE, Some(Num::int(6)), false),
        ),
        (
            "dart",
            projectile(false, Num::ZERO, Some(Num::int(6)), true),
        ),
        (
            "scout",
            ProjectileData {
                hits: ProjectileHits::Nothing,
                ..projectile(false, Num::ZERO, Some(Num::int(6)), false)
            },
        ),
    ];
    let [bolt, lance, dart, scout] = types.map(|(name, data)| {
        let unit_type = game.load_unit_type(TypeScope::Mode, name, &UnitTypeData::default());
        game.load_projectile_type(unit_type, &data);
        unit_type
    });
    game.insert_resource(Volley {
        bolt,
        lance,
        dart,
        scout,
    });
    game
}

#[test]
fn a_projectile_flies_to_its_target_and_strikes_as_it_reaches_its_body() {
    let mut game = volley_match();
    let bolt = game.volley().bolt;
    let shooter_id = game.spawn_armed(Team::new(0), at(0, 0), shooter(bolt));
    let target_id = game.spawn_armed(Team::new(1), at(5, 0), target());
    let body = Body::new(Num::ONE).unwrap();
    let entity = game.unit_entity(target_id);
    game.insert_parts(target_id, body);
    game.attack(shooter_id, target_id);

    // The attack starts in tick 0 and fires in tick 2, from the origin. The projectile, of no
    // width, flies from tick 3, half a meter a tick: the target's body of 1 m at 5 m starts at
    // 4 m, 8 steps, ticks 3 to 10, and it strikes in the last, which it ends on. The next attack
    // starts in tick 10 and fires in tick 12.
    let mut flights = Vec::new();
    let mut healths = Vec::new();
    for _ in 0..=12 {
        game.step();
        flights.push(game.projectiles());
        healths.push(game.health(target_id));
    }
    let flying = |halves: i64| {
        let x = Num::from_bits(halves << (Num::FRAC_BITS - 1));
        vec![Position::new(Vec3::new(x, Num::ZERO, Num::ZERO)).unwrap()]
    };
    let expected: Vec<_> = [vec![], vec![]]
        .into_iter()
        .chain((0..=7).map(flying))
        .chain([vec![], vec![], flying(0)])
        .collect();
    assert_eq!(flights, expected);
    assert_eq!(healths, [[100; 10].as_slice(), &[70; 3]].concat());
    let attackers = game.get::<RecentAttackers>(entity).unwrap();
    let attack = RecentAttack {
        source: shooter_id,
        tick: Tick::new(10),
    };
    assert_eq!(attackers.iter().collect::<Vec<_>>(), [attack]);

    // The one in flight, fired in tick 12, carries the roll its attack drew as its windup
    // ended: the first draw of its attacker's roll stream in that tick, on the match's seed.
    let mut source = RngSource::new(SegmentSeed::new([0; 32]));
    source.begin_tick(12);
    let roll = source.open(ROLL_STREAM, shooter_id.get()).fraction();
    assert_eq!(game.rolls(), [roll]);
}

#[test]
fn a_projectile_whose_target_dies_or_goes_first_ends_without_a_hit() {
    let mut game = volley_match();
    let bolt = game.volley().bolt;
    let first = game.spawn_armed(Team::new(0), at(0, 0), shooter(bolt));
    let second = game.spawn_armed(Team::new(0), at(0, 1), shooter(bolt));
    let doomed = game.spawn_armed(Team::new(1), at(5, 0), target());
    let gone = game.spawn_armed(Team::new(1), at(-5, 1), target());
    game.attack(first, doomed);
    game.attack(second, gone);

    // Both fire in tick 2. Before tick 5 one target dies and the other despawns: in tick 5 both
    // projectiles end where they were, and neither strikes.
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(game.projectiles().len(), 2);
    game.insert_parts(doomed, Dead);
    let entity = game.unit_entity(gone);
    game.despawn(entity);

    // A projectile is state while it flies, and so are the units a cast struck.
    let hits = [first, second].map(|by| Struck { by, id: doomed });
    let bytes = Binary::encode(&hits[..]);
    let struck = Binary::decode::<StruckUnits>(&bytes).unwrap();
    assert!(hits.iter().all(|&hit| struck.contains(hit)));
    game.insert_resource(struck);
    // A restore loads the match's books first: the same weapons, in the same order.
    let fresh = || {
        let mut restored = volley_match();
        for armed in [
            shooter(restored.volley().bolt),
            shooter(restored.volley().bolt),
            target(),
            target(),
        ] {
            let _weapon = armed.bundle(&mut restored);
        }
        restored
    };
    let mut restored = fresh();
    game.restore_into(&mut restored);
    assert_eq!(restored.projectiles(), game.projectiles());
    let struck = |game: &Match| game.resource::<StruckUnits>().clone();
    assert_eq!(struck(&restored), struck(&game));
    // A projectile without its flight state never flies, so a snapshot of one is refused.
    let bolt = restored
        .query_filtered::<Entity, With<Projectile>>()
        .iter(&restored)
        .next()
        .unwrap();
    restored.entity_mut(bolt).remove::<FlightState>();
    let id = *restored.get::<StableId>(bolt).unwrap();
    let refusal = restored.refusal_into(&mut fresh());
    let lacks = SnapshotError::Lacks {
        holder: Projectile::NAME,
        needs: FlightState::NAME,
        id,
    };
    assert_eq!(refusal, Some(lacks));
    // The struck units decode only in order, each once: the ids were allocated in order.
    let decode = |hits: &[(StableId, StableId)]| {
        let hits: Vec<Struck> = hits.iter().map(|&(by, id)| Struck { by, id }).collect();
        let bytes = Binary::encode(&hits);
        Binary::decode::<StruckUnits>(&bytes)
    };
    assert!(decode(&[(first, doomed), (first, gone), (second, doomed)]).is_ok());
    assert!(decode(&[(first, gone), (first, doomed)]).is_err());
    assert!(decode(&[(first, doomed), (first, doomed)]).is_err());

    game.step();
    assert_eq!(game.now().get(), 6);
    assert_eq!(game.projectiles(), []);
    assert_eq!(game.health(doomed), 100);
}

#[test]
fn a_teleport_ends_the_homing_projectiles_on_its_unit_and_a_dash_does_not() {
    let mut game = volley_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Projectiles,
    ]);
    let bolt = game.volley().bolt;
    let first = game.spawn_armed(Team::new(0), at(0, 0), shooter(bolt));
    let second = game.spawn_armed(Team::new(0), at(0, 1), shooter(bolt));
    let blinker = game.spawn_armed(Team::new(1), at(5, 0), target());
    let dasher = game.spawn_armed(Team::new(1), at(5, 1), target());
    game.attack(first, blinker);
    game.attack(second, dasher);

    // Both fire in tick 2 and fly from tick 3. Before tick 5 one target teleports to (6, 0), and
    // the other dashes to (6, 1), half a meter a tick. In tick 5 the projectile on the first ends
    // where it was, with no hit; the one on the second flies on, 6 m from tick 3 at half a meter
    // a tick, and strikes it in tick 14, where it stands since tick 6. Neither shooter fires
    // again before tick 22.
    game.run_until(5);
    assert_eq!(game.projectiles().len(), 2);
    let now = game.now();
    let blink = NavigationEffect::Teleport {
        id: blinker,
        to: at(6, 0),
    };
    let dash = NavigationEffect::Dash {
        id: dasher,
        to: DashTo::Point(at(6, 1)),
        step: Num::from_bits(1 << (Num::FRAC_BITS - 1)),
        delivers: None,
    };
    for effect in [blink, dash] {
        effect.apply(&mut game, &mut Frame::in_tick(now));
    }
    game.step();
    let flying = game
        .query::<(&Projectile, &FlightState)>()
        .iter(&game)
        .map(|(projectile, state)| (projectile.flight(), state.lost()))
        .collect::<Vec<_>>();
    assert!(matches!(flying[..], [(Flight::Homing { target }, false)] if target == dasher));
    game.run_until(14);
    assert_eq!([blinker, dasher].map(|id| game.health(id)), [100, 100]);
    game.step();
    assert_eq!([blinker, dasher].map(|id| game.health(id)), [100, 70]);
    assert_eq!(*game.part::<Position>(dasher), at(6, 1));
}

#[test]
fn a_projectile_that_outlives_its_source_kills_with_no_killer() {
    // Two shooters fire at a target of 60 health in tick 2, from 5 m either side: both projectiles
    // strike in tick 12, 30 damage each, the lower id's first. The first shooter despawns in tick
    // 5, as a dead creep does, so its strike has no source that exists: the second one's strike
    // kills, and it alone is the killer's. Were the second shooter gone too, no one would be.
    for second_goes in [false, true] {
        let mut game = volley_match();
        let bolt = game.volley().bolt;
        let shooters = [0, 10].map(|x| game.spawn_armed(Team::new(0), at(x, 0), shooter(bolt)));
        let victim = game.spawn_armed(Team::new(1), at(5, 0), target());
        let victim_entity = game.unit_entity(victim);
        let mut life = game.get_mut::<Life>(victim_entity).unwrap();
        life.take(Num::int(40));
        for shooter in shooters {
            game.attack(shooter, victim);
        }
        for tick in 0..=12 {
            if tick == 5 {
                let gone: &[StableId] = if second_goes {
                    &shooters
                } else {
                    &shooters[..1]
                };
                for &id in gone {
                    let entity = game.unit_entity(id);
                    game.despawn(entity);
                }
            }
            game.step();
        }
        assert_eq!(game.health(victim), 0);
        let deaths: Vec<_> = game
            .resource::<Deaths>()
            .iter()
            .map(|death| (death.fallen.id, death.killer, death.assisters.to_vec()))
            .collect();
        let killer = (!second_goes).then_some(shooters[1]);
        assert_eq!(
            deaths,
            [(victim, killer, vec![])],
            "second goes: {second_goes}"
        );
        // Only a source that exists is an attacker.
        let attackers: Vec<_> = game
            .get::<RecentAttackers>(victim_entity)
            .unwrap()
            .iter()
            .map(|attack| attack.source)
            .collect();
        assert_eq!(attackers, killer.into_iter().collect::<Vec<_>>());
    }
}

#[test]
fn a_line_projectile_hits_each_enemy_its_path_comes_within_reach_of_once_and_ends_at_its_range() {
    let mut game = volley_match();
    let source = game.spawn_armed(Team::new(0), at(0, -9), target());
    let point = |x: Num, z: Num| Position::new(Vec3::new(x, Num::ZERO, z)).unwrap();
    let e = Num::EPSILON;
    // The lance's reach is half its width, 0.5 m, and these bodies have no radius. Its path runs
    // along x from 0 to 6: 0.5 m beside it and 0.5 m past its end are in reach, one bit more is
    // not, and an ally is never hit.
    let beside = game.spawn_armed(Team::new(1), point(Num::int(2), Num::HALF), target());
    let wide = game.spawn_armed(Team::new(1), point(Num::int(3), Num::HALF + e), target());
    let ally = game.spawn_armed(Team::new(0), at(1, 0), target());
    let past = game.spawn_armed(
        Team::new(1),
        point(Num::int(6) + Num::HALF, Num::ZERO),
        target(),
    );
    let beyond = game.spawn_armed(
        Team::new(1),
        point(Num::int(6) + Num::HALF + e, Num::ZERO),
        target(),
    );
    let lance = game.volley().lance;
    game.fire_line(source, lance);

    // It launches in tick 0 and flies half a meter a tick from tick 1: 6 m is ticks 1 to 12, and
    // it ends in tick 12, as its range runs out. After tick 11 it is at 5.5 m.
    for _ in 0..12 {
        game.step();
    }
    assert_eq!(
        game.projectiles(),
        [point(Num::int(5) + Num::HALF, Num::ZERO)]
    );
    // Its state holds the 5.5 m it flew. A line flies within its range, 6 m, and loses no unit,
    // and a state of fewer than 0 m fails to decode.
    let (entity, &state) = game
        .query::<(Entity, &FlightState)>()
        .single(&game)
        .unwrap();
    assert_eq!(state.flown(), Num::int(5) + Num::HALF);
    let flown = |meters| {
        let mut state = FlightState::START;
        state.fly_to(meters);
        state
    };
    let mut lost = flown(Num::ONE);
    lost.disjoint();
    let checks =
        [flown(Num::int(6)), flown(Num::int(6) + e), lost].map(|state| state.check(&game, entity));
    assert_eq!(checks, [true, false, false]);
    let decodes =
        |meters: Num| Binary::decode::<FlightState>(&Binary::encode(&(meters, false))).is_ok();
    assert_eq!([Num::ZERO, -e].map(decodes), [true, false]);
    game.step();
    assert_eq!(game.projectiles(), []);
    let healths = [beside, wide, ally, past, beyond].map(|id| game.health(id));
    assert_eq!(healths, [90, 100, 100, 90, 100]);

    // A dart of no width ends at its first hit: of two enemies on one point, the lower id, in
    // tick 6, where its path reaches 3 m from 2.5 m; the one past them is never hit.
    let mut game = volley_match();
    let source = game.spawn_armed(Team::new(0), at(0, -9), target());
    let [first, second, behind] =
        [at(3, 0), at(3, 0), at(4, 0)].map(|at| game.spawn_armed(Team::new(1), at, target()));
    let dart = game.volley().dart;
    game.fire_line(source, dart);
    for _ in 0..6 {
        game.step();
    }
    assert_eq!(
        game.projectiles(),
        [point(Num::int(2) + Num::HALF, Num::ZERO)]
    );
    game.step();
    assert_eq!(game.projectiles(), []);
    let healths = [first, second, behind].map(|id| game.health(id));
    assert_eq!(healths, [90, 100, 100]);

    // An enemy with a box of 2 by 1 m at (3.5, 0.3), whose near edge, x = 2.5, the dart's path
    // enters before it comes to a point enemy at 2.6: the box is met first, where the path enters
    // it, though its center lies past the point, in tick 6, and the dart ends on it.
    let mut game = volley_match();
    let source = game.spawn_armed(Team::new(0), at(0, -9), target());
    let near = |x: &str, z: &str| point(x.parse().unwrap(), z.parse().unwrap());
    let block = game.spawn_armed(Team::new(1), near("3.5", "0.3"), target());
    let body = BodyBox::new([Num::int(2), Num::ONE], Num::ZERO).unwrap();
    game.insert_parts(block, Body::of_box(body));
    let dot = game.spawn_armed(Team::new(1), near("2.6", "0"), target());
    game.fire_line(source, dart);
    for _ in 0..7 {
        game.step();
    }
    assert_eq!(game.projectiles(), []);
    let healths = [block, dot].map(|id| game.health(id));
    assert_eq!(healths, [90, 100]);
}

#[test]
fn a_projectile_that_hits_nothing_crosses_bodies_and_one_with_sight_reveals_where_it_flies() {
    // A scout of team 0, which hits nothing, flies from the origin along x, half a meter a tick
    // from tick 1, for 6 m, and sees 1 m: its type's vision section, which the delivery spawner
    // gives its unit. An enemy of team 1 stands on its path at (5, 0), in the cell whose center is
    // (5.5, 0.5); the scout's sight reaches that center from x within √0.75 ≈ 0.87 m of 5.5, so
    // after ticks 10 and 11, at 5 m and 5.5 m. It ends at its range in tick 12, and despawns.
    let mut game = volley_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Projectiles,
        Capability::Vision,
    ]);
    let scout = game.volley().scout;
    let bounds = Bounds::new([Num::int(-10), Num::int(-10)], [Num::int(10), Num::int(10)]).unwrap();
    game.load_vision_grid(Grid::new(Num::ONE, bounds).unwrap(), &[], 2);
    let sight = Sight::new(Num::ONE).unwrap();
    game.resource_mut::<ByType<Sight>>().set(scout, sight);
    let source = game.spawn_armed(Team::new(0), at(0, -9), target());
    let enemy = game.spawn_armed(Team::new(1), at(5, 0), target());
    game.fire_line(source, scout);
    let seen = |game: &Match| game.part::<SeenBy>(enemy).get().contains(Team::new(0));
    let mut seen_in = Vec::new();
    for tick in 0..14 {
        game.step();
        if seen(&game) {
            seen_in.push(tick);
        }
    }
    assert_eq!(seen_in, [10, 11]);
    assert_eq!(game.projectiles(), []);
    assert_eq!(game.health(enemy), 100);
}

#[test]
fn a_projectile_reads_only_with_a_positive_speed_and_no_negative_width_or_range() {
    let read = |text: &str| Toml::parse::<ProjectileData>(text);
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    let plain = ProjectileData::flying(Num::HALF);
    assert_eq!(read("speed = \"0.5\"").unwrap(), plain);
    assert_eq!(
        read("speed = 20\nwidth = 0\nrange = 0").unwrap(),
        ProjectileData {
            speed: Num::int(20),
            range: Some(Num::ZERO),
            ..plain.clone()
        }
    );
    // `hits` is `none` or a filter, `enemies` when it is absent.
    let hits = |text: &str| read(&format!("speed = 1\nhits = {text}")).map(|data| data.hits);
    assert_eq!(hits("\"none\"").unwrap(), ProjectileHits::Nothing);
    assert_eq!(
        hits("\"allies:avatar\"").unwrap(),
        ProjectileHits::Units(FilterData::parse("allies:avatar").unwrap())
    );
    assert_eq!(plain.hits, ProjectileHits::ENEMIES);
    assert!(
        hits("\"nobody\"")
            .unwrap_err()
            .message()
            .starts_with("filter \"nobody\"")
    );
    for (text, message) in [
        ("speed = 0", "a projectile's speed is positive"),
        ("speed = \"-0.5\"", "a projectile's speed is positive"),
        (
            "speed = 9223372036854775807",
            "a projectile's speed is positive",
        ),
        (
            "speed = 20\nwidth = -1",
            "a projectile's width is not negative",
        ),
        (
            "speed = 20\nwidth = \"-0.001\"",
            "a projectile's width is not negative",
        ),
        (
            "speed = 20\nrange = -1",
            "a projectile's range is not negative",
        ),
    ] {
        assert!(refusal(text).starts_with(message), "{text}");
    }
}
