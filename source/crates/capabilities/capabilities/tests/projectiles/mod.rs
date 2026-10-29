use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use campfire_actions::LaunchId;
use campfire_capabilities::{Match, Release};
use campfire_codec::{Binary, BinaryError};
use campfire_combat::internals::Armed;
use campfire_combat::{Combat, OnDeath, ROLL_STREAM, RecentAttack, RecentAttackers};
use campfire_common::{SegmentSeed, Tick};
use campfire_declarations::ById;
use campfire_declarations::internals::BookLoad;
use campfire_deliveries::Delivering;
use campfire_geometry::{Bounds, Grid};
use campfire_math::{Num, Rng, Vec3};
use campfire_navigation::NavigationEffect;
use campfire_projectiles::*;
use campfire_script::{Effect, Frame};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimComponent, SimWorld, SnapshotError, StableId};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{
    BodyForm, BodyShape, DashTo, Dead, Deaths, Layer, Life, MapBook, SeenBy, UnitTypeCore, Units,
};
use campfire_values::{ActionId, FilterData, Rank, Team, TypeScope, UnitType};
use campfire_vision::{Sight, Vision};

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
pub(super) fn target() -> Armed {
    Armed::unarmed(Num::int(100)).on_death(OnDeath::Stay)
}

/// A match with combat and projectiles.
pub(super) fn volley_match() -> Match {
    volley_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Projectiles,
    ])
}

/// The unit vector along +x, the way `Projectiles::fire_line` fires most lines.
const ALONG_X: Vec3 = Vec3::new(Num::ONE, Num::ZERO, Num::ZERO);

/// A match of `declared`, with combat and projectiles among them, and the stat book a weapon
/// reads.
fn volley_match_of(declared: &[Capability]) -> Match {
    let mut game = Match::builder(declared).build();
    Combat::load_weapon_stats(&mut game);
    game
}

/// The projectile and unit types a volley's match loads.
#[derive(Debug, Clone, Copy)]
struct Volley {
    /// A shooter of bolts.
    shooter: UnitType,
    /// A target of no body.
    target: UnitType,
    /// A target of a body of 1 m.
    round: UnitType,
    /// A target of a box of 2 by 1 m.
    block: UnitType,
    /// A line of 1 m wide for 6 m, that passes through what it hits.
    lance: UnitType,
    /// A line of no width for 6 m, that ends at its first hit.
    dart: UnitType,
    /// A line for 6 m that hits nothing.
    scout: UnitType,
}

impl Volley {
    /// Loads the projectile types into `game`, a match with combat and projectiles, then the
    /// unit types.
    fn place(game: &mut Match) -> Self {
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
            let unit_type = Units::load_type(game, TypeScope::Mode, name, &UnitTypeCore::default());
            Projectiles::load_type(game, unit_type, &data);
            unit_type
        });
        let shooter = shooter(bolt).load(game);
        let [target, round, block] = [(); 3].map(|()| target().load(game));
        Units::give_type(
            game,
            round,
            BodyForm::new(BodyShape::circle(Num::ONE).unwrap(), Layer::FIRST),
        );
        let block_body = BodyForm::new(
            BodyShape::box_sized([Num::int(2), Num::ONE]).unwrap(),
            Layer::FIRST,
        );
        Units::give_type(game, block, block_body);
        Self {
            shooter,
            target,
            round,
            block,
            lance,
            dart,
            scout,
        }
    }
}

/// Moves the unit `id` of `game` to `position`.
fn put(game: &mut Match, id: StableId, position: Vec3) {
    let entity = game.unit_entity(id);
    *game.get_mut::<Position>(entity).unwrap() = Position::from(position);
}

#[test]
fn a_projectile_flies_to_its_target_and_strikes_as_it_reaches_its_body() {
    let mut game = volley_match();
    let volley = Volley::place(&mut game);
    let shooter_id = Spawn::of(volley.shooter, (0, 0, 0)).place(&mut game);
    let target_id = Spawn::of(volley.round, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let entity = game.unit_entity(target_id);
    Combat::attack(&mut game, shooter_id, target_id);

    // The attack starts in tick 0 and fires in tick 2, from the origin. The projectile, of no
    // width, flies from tick 3, half a meter a tick: the target's body of 1 m at 5 m starts at
    // 4 m, 8 steps, ticks 3 to 10, and it strikes in the last, which it ends on. The next attack
    // starts in tick 10 and fires in tick 12.
    let mut flights = Vec::new();
    let mut healths = Vec::new();
    for _ in 0..=12 {
        game.step();
        flights.push(Projectiles::projectiles(&game));
        healths.push(Stats::health(&game, target_id));
    }
    let flying = |halves: i64| {
        let x = Num::from_bits(halves << (Num::FRAC_BITS - 1));
        vec![Vec3::new(x, Num::ZERO, Num::ZERO)]
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
    let seed = SegmentSeed::ZERO;
    let roll = Rng::new(&seed, ROLL_STREAM, shooter_id.get(), Tick::new(12)).fraction();
    assert_eq!(Projectiles::rolls(&game), [roll]);
}

#[test]
fn a_projectile_whose_target_dies_or_goes_first_ends_without_a_hit() {
    let mut game = volley_match();
    let volley = Volley::place(&mut game);
    let first = Spawn::of(volley.shooter, (0, 0, 0)).place(&mut game);
    let second = Spawn::of(volley.shooter, (0, 0, 1)).place(&mut game);
    let doomed = Spawn::of(volley.target, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let gone = Spawn::of(volley.target, (-5, 0, 1))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, first, doomed);
    Combat::attack(&mut game, second, gone);

    // Both fire in tick 2. Before tick 5 one target dies and the other despawns: in tick 5 both
    // projectiles end where they were, and neither strikes.
    game.steps(5);
    assert_eq!(Projectiles::projectiles(&game).len(), 2);
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
        Volley::place(&mut restored);
        restored
    };
    let mut restored = fresh();
    Release::restore_into(&game, &mut restored);
    assert_eq!(
        Projectiles::projectiles(&restored),
        Projectiles::projectiles(&game)
    );
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
    let refusal = Release::refusal_into(&restored, &mut fresh());
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
    assert_eq!(
        decode(&[(first, gone), (first, doomed)]).err(),
        Some(BinaryError::REFUSED)
    );
    assert_eq!(
        decode(&[(first, doomed), (first, doomed)]).err(),
        Some(BinaryError::REFUSED)
    );

    game.step();
    assert_eq!(game.now().get(), 6);
    assert_eq!(Projectiles::projectiles(&game), []);
    assert_eq!(Stats::health(&game, doomed), 100);
}

#[test]
fn a_teleport_ends_the_homing_projectiles_on_its_unit_and_a_dash_does_not() {
    let mut game = volley_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Projectiles,
    ]);
    let volley = Volley::place(&mut game);
    let first = Spawn::of(volley.shooter, (0, 0, 0)).place(&mut game);
    let second = Spawn::of(volley.shooter, (0, 0, 1)).place(&mut game);
    let blinker = Spawn::of(volley.target, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let dasher = Spawn::of(volley.target, (5, 0, 1))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, first, blinker);
    Combat::attack(&mut game, second, dasher);

    // Both fire in tick 2 and fly from tick 3. Before tick 5 one target teleports to (6, 0), and
    // the other dashes to (6, 1), half a meter a tick. In tick 5 the projectile on the first ends
    // where it was, with no hit; the one on the second flies on, 6 m from tick 3 at half a meter
    // a tick, and strikes it in tick 14, where it stands since tick 6. Neither shooter fires
    // again before tick 22.
    game.run_until(5);
    assert_eq!(Projectiles::projectiles(&game).len(), 2);
    let now = game.now();
    let blink = NavigationEffect::Teleport {
        id: blinker,
        to: Vec3::from((6, 0, 0)),
    };
    let dash = NavigationEffect::Dash {
        id: dasher,
        to: DashTo::Point(Vec3::from((6, 0, 1))),
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
        .map(|(projectile, state)| (projectile.flight(), state.is_lost()))
        .collect::<Vec<_>>();
    assert!(matches!(flying[..], [(Flight::Homing { target }, false)] if target == dasher));
    game.run_until(14);
    assert_eq!(
        [blinker, dasher].map(|id| Stats::health(&game, id)),
        [100, 100]
    );
    game.step();
    assert_eq!(
        [blinker, dasher].map(|id| Stats::health(&game, id)),
        [100, 70]
    );
    assert_eq!(game.part::<Position>(dasher).get(), Vec3::from((6, 0, 1)));
}

#[test]
fn a_projectile_that_outlives_its_source_kills_with_no_killer() {
    // Two shooters fire at a target of 60 health in tick 2, from 5 m either side: both projectiles
    // strike in tick 12, 30 damage each, the lower id's first. The first shooter despawns in tick
    // 5, as a dead creep does, so its strike has no source that exists: the second one's strike
    // kills, and it alone is the killer's. Were the second shooter gone too, no one would be.
    for second_goes in [false, true] {
        let mut game = volley_match();
        let volley = Volley::place(&mut game);
        let shooters = [0, 10].map(|x| Spawn::of(volley.shooter, (x, 0, 0)).place(&mut game));
        let victim = Spawn::of(volley.target, (5, 0, 0))
            .team(Team::new(1))
            .place(&mut game);
        let victim_entity = game.unit_entity(victim);
        let mut life = game.get_mut::<Life>(victim_entity).unwrap();
        life.take(Num::int(40));
        for shooter in shooters {
            Combat::attack(&mut game, shooter, victim);
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
        assert_eq!(Stats::health(&game, victim), 0);
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
    let volley = Volley::place(&mut game);
    let source = Spawn::of(volley.target, (0, 0, -9)).place(&mut game);
    let point = |x: Num, z: Num| Vec3::new(x, Num::ZERO, z);
    let e = Num::EPSILON;
    // The lance's reach is half its width, 0.5 m, and these bodies have no radius. Its path runs
    // along x from 0 to 6: 0.5 m beside it and 0.5 m past its end are in reach, one bit more is
    // not, and an ally is never hit.
    let beside = Spawn::of(volley.target, point(Num::int(2), Num::HALF))
        .team(Team::new(1))
        .place(&mut game);
    let wide = Spawn::of(volley.target, point(Num::int(3), Num::HALF + e))
        .team(Team::new(1))
        .place(&mut game);
    let ally = Spawn::of(volley.target, (1, 0, 0)).place(&mut game);
    let past = Spawn::of(volley.target, point(Num::int(6) + Num::HALF, Num::ZERO))
        .team(Team::new(1))
        .place(&mut game);
    let beyond = Spawn::of(volley.target, point(Num::int(6) + Num::HALF + e, Num::ZERO))
        .team(Team::new(1))
        .place(&mut game);
    let lance = volley.lance;
    Projectiles::fire_line(&mut game, source, lance, ALONG_X);

    // It launches in tick 0 and flies half a meter a tick from tick 1: 6 m is ticks 1 to 12, and
    // it ends in tick 12, as its range runs out. After tick 11 it is at 5.5 m.
    game.steps(12);
    assert_eq!(
        Projectiles::projectiles(&game),
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
    let decodes = |flown: Num| {
        let state = FlightState::Flown { flown, lost: false };
        Binary::decode::<FlightState>(&Binary::encode(&state)).is_ok()
    };
    assert_eq!([Num::ZERO, -e].map(decodes), [true, false]);
    game.step();
    assert_eq!(Projectiles::projectiles(&game), []);
    let healths = [beside, wide, ally, past, beyond].map(|id| Stats::health(&game, id));
    assert_eq!(healths, [90, 100, 100, 90, 100]);

    // A dart of no width ends at its first hit: of two enemies on one point, the lower id, in
    // tick 6, where its path reaches 3 m from 2.5 m; the one past them is never hit.
    let mut game = volley_match();
    let volley = Volley::place(&mut game);
    let source = Spawn::of(volley.target, (0, 0, -9)).place(&mut game);
    let [first, second, behind] = [
        Vec3::from((3, 0, 0)),
        Vec3::from((3, 0, 0)),
        Vec3::from((4, 0, 0)),
    ]
    .map(|at| {
        Spawn::of(volley.target, at)
            .team(Team::new(1))
            .place(&mut game)
    });
    let dart = volley.dart;
    Projectiles::fire_line(&mut game, source, dart, ALONG_X);
    game.steps(6);
    assert_eq!(
        Projectiles::projectiles(&game),
        [point(Num::int(2) + Num::HALF, Num::ZERO)]
    );
    game.step();
    assert_eq!(Projectiles::projectiles(&game), []);
    let healths = [first, second, behind].map(|id| Stats::health(&game, id));
    assert_eq!(healths, [90, 100, 100]);

    // An enemy with a box of 2 by 1 m at (3.5, 0.3), whose near edge, x = 2.5, the dart's path
    // enters before it comes to a point enemy at 2.6: the box is met first, where the path enters
    // it, though its center lies past the point, in tick 6, and the dart ends on it. A match
    // with no navigation spawns the box, which makes room for none.
    let mut game = volley_match();
    let volley = Volley::place(&mut game);
    let source = Spawn::of(volley.target, (0, 0, -9)).place(&mut game);
    let near = |x: &str, z: &str| point(x.parse().unwrap(), z.parse().unwrap());
    let block = Spawn::of(volley.block, near("3.5", "0.3"))
        .team(Team::new(1))
        .place(&mut game);
    let dot = Spawn::of(volley.target, near("2.6", "0"))
        .team(Team::new(1))
        .place(&mut game);
    Projectiles::fire_line(&mut game, source, dart, ALONG_X);
    game.steps(7);
    assert_eq!(Projectiles::projectiles(&game), []);
    let healths = [block, dot].map(|id| Stats::health(&game, id));
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
    let volley = Volley::place(&mut game);
    let scout = volley.scout;
    let bounds = Bounds::new([Num::int(-10), Num::int(-10)], [Num::int(10), Num::int(10)]).unwrap();
    Vision::load_grid(&mut game, Grid::new(Num::ONE, bounds).unwrap(), &[], 2);
    let sight = Sight::new(Num::ONE).unwrap();
    ById::<UnitType, Sight>::amend(&mut game, |_, book| book.set(scout, sight));
    let source = Spawn::of(volley.target, (0, 0, -9)).place(&mut game);
    let enemy = Spawn::of(volley.target, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Projectiles::fire_line(&mut game, source, scout, ALONG_X);
    let seen = |game: &Match| {
        game.part::<SeenBy>(enemy)
            .get()
            .unwrap()
            .contains(Team::new(0))
    };
    let mut seen_in = Vec::new();
    for tick in 0..14 {
        game.step();
        if seen(&game) {
            seen_in.push(tick);
        }
    }
    assert_eq!(seen_in, [10, 11]);
    assert_eq!(Projectiles::projectiles(&game), []);
    assert_eq!(Stats::health(&game, enemy), 100);
}

#[test]
fn a_projectile_carries_the_launch_its_delivery_belongs_to() {
    // Two scouts of action 0 launch along x, one for the action's own delivery and one for its
    // listed launch 0: each projectile carries what it was launched for, so the second's hits
    // and its end run the launch's lists and not the action's.
    let mut game = volley_match();
    let volley = Volley::place(&mut game);
    let source = Spawn::of(volley.target, (0, 0, -9)).place(&mut game);
    let by = |launch| Delivering {
        source,
        action: ActionId::new(0),
        rank: Rank::FIRST,
        start: None,
        launch,
    };
    Projectiles::deliver_line(&mut game, by(None), volley.scout, ALONG_X);
    Projectiles::deliver_line(&mut game, by(Some(LaunchId::nth(0))), volley.scout, ALONG_X);
    game.step();
    assert_eq!(
        Projectiles::carried_launches(&game),
        [None, Some(LaunchId::nth(0))]
    );
}

#[test]
fn a_line_that_reaches_the_bounds_ends_on_their_edge_and_its_sight_stays_within_them() {
    // The scout flies 6 m from the origin along (0.6, 0, 0.8), each axis rounded once, toward
    // the edge at x = 3, with sight over a grid of the bounds, which `see` takes every unit to
    // stand within. The edge lies 3 / 0.600 000 02 = 4.999 999 8 m on, 83 886 076.67 bits,
    // rounded down, as `Bounds::exit` takes it: half a meter a tick, it flies 4.5 m, then the
    // last 0.499 999 8 m to that place, x = 2.999 999 95, z = 3.999 999 8, and ends there. Each
    // place is the origin plus the direction times the distance, rounded once: a step added to
    // the last place would carry each step's rounding on, 4 bits short in z by 4.5 m.
    let mut game = volley_match_of(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Projectiles,
        Capability::Vision,
    ]);
    let volley = Volley::place(&mut game);
    let scout = volley.scout;
    let bounds = Bounds::new([Num::int(-5), Num::int(-5)], [Num::int(3), Num::int(5)]).unwrap();
    MapBook::amend(&mut game, |_, map| map.bounds = bounds);
    Vision::load_grid(&mut game, Grid::new(Num::ONE, bounds).unwrap(), &[], 2);
    let sight = Sight::new(Num::ONE).unwrap();
    ById::<UnitType, Sight>::amend(&mut game, |_, book| book.set(scout, sight));
    let source = Spawn::of(volley.target, (0, 0, -4)).place(&mut game);
    let slant = Vec3::new(
        Num::from_bits(10_066_330),
        Num::ZERO,
        Num::from_bits(13_421_773),
    );
    Projectiles::fire_line(&mut game, source, scout, slant);
    let mut flown = Vec::new();
    for _ in 0..12 {
        game.step();
        flown.extend(Projectiles::projectiles(&game));
    }
    // Aloft at the origin as it launches, then 0.5 m on each tick to 4.5 m: 4.5 · 10 066 330 =
    // 45 298 485 bits, and 4.5 · 13 421 773 = 60 397 978.5, to the even 60 397 978.
    assert_eq!(flown.len(), 10);
    assert_eq!(flown[0], Vec3::ZERO);
    let last = flown[9].to_bits();
    assert_eq!([last[0], last[2]], [45_298_485, 60_397_978]);
    assert!(flown.iter().all(|&at| bounds.contains(at)));
}

mod curves;
mod deliveries;
mod steered;
