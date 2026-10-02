use campfire_common::{SegmentSeed, Tick};
use campfire_math::RngSource;
use campfire_sim::{Capability, EntityIndex};

use super::*;
use crate::actions::action_slots::ActionSlots;
use crate::capability_set::test_match::TestMatch;
use crate::combat::ROLL_STREAM;
use crate::combat::deaths::Deaths;
use crate::combat::internals::Armed;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attack::RecentAttack;
use crate::combat::recent_attackers::RecentAttackers;
use crate::projectiles::projectile_data::ProjectileData;
use crate::projectiles::struck_units::Struck;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::pool_id::PoolId;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::damage_kind::DamageKind;
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

#[derive(Debug)]
struct Volley {
    sim: TestMatch,
    /// A homing projectile type.
    bolt: UnitType,
    /// A line of 1 m wide for 6 m, that passes through what it hits.
    lance: UnitType,
    /// A line of no width for 6 m, that ends at its first hit.
    dart: UnitType,
}

impl Volley {
    /// A match with combat and projectiles, and the projectiles' types.
    fn new() -> Volley {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Projectiles,
        ];
        let budgets = ScriptBudgets::new(ScriptLimits::ROOMY, 1);
        let mut sim = TestMatch::server(&declared, budgets);
        let world = &mut sim.world;
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
        ];
        let [bolt, lance, dart] = types.map(|(name, data)| {
            let unit_type =
                Units::load_type(world, TypeScope::Mode, name, &UnitTypeData::default());
            Projectiles::load_type(world, unit_type, &data);
            unit_type
        });
        Volley {
            sim,
            bolt,
            lance,
            dart,
        }
    }

    /// Launches a projectile of the line type `unit_type` from `source` at the origin along +x,
    /// carrying 10 damage, from the next tick.
    fn fire_line(&mut self, source: StableId, unit_type: UnitType) {
        let range = self
            .sim
            .world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .unwrap()
            .range
            .unwrap();
        let mut launches = self.sim.world.resource_mut::<Launches>();
        launches.launches.push(Launch {
            source,
            from: at(0, 0),
            unit_type,
            flight: Flight::Line {
                direction: Vec3::new(Num::ONE, Num::ZERO, Num::ZERO),
                flown: Num::ZERO,
                range,
                aimed: None,
            },
            payload: LaunchPayload::Attack {
                action: ActionId::nth(0),
                amount: Num::int(10),
                kind: DamageKind::new(0),
                roll: Num::ZERO,
            },
        });
    }

    fn unit(&mut self, team: u8, at: Position, combatant: Armed) -> StableId {
        let bundle = combatant.bundle(&mut self.sim.world, Team::new(team));
        self.sim.spawn(at, bundle)
    }

    fn attack(&mut self, attacker: StableId, target: StableId) {
        let mut slots = self.sim.get_mut::<ActionSlots>(attacker);
        slots.set_attack_target(Some(target));
    }

    /// The roll each projectile carries, by stable id.
    fn rolls(&self) -> Vec<Num> {
        let world = &self.sim.world;
        let projectiles = world.resource::<EntityIndex>().iter();
        projectiles
            .filter_map(|(_, entity)| world.get::<Projectile>(entity))
            .map(|projectile| match projectile.payload() {
                Payload::Attack { roll, .. } => roll,
                Payload::Action { .. } => panic!("an attack's projectile"),
            })
            .collect()
    }

    /// Where each projectile is, by stable id.
    fn projectiles(&self) -> Vec<Position> {
        let world = &self.sim.world;
        world
            .resource::<EntityIndex>()
            .iter()
            .filter(|&(_, entity)| world.get::<Projectile>(entity).is_some())
            .map(|(_, entity)| *world.get::<Position>(entity).unwrap())
            .collect()
    }
}

#[test]
fn a_projectile_flies_to_its_target_and_strikes_as_it_reaches_its_body() {
    let mut volley = Volley::new();
    let shooter_id = volley.unit(0, at(0, 0), shooter(volley.bolt));
    let target_id = volley.unit(1, at(5, 0), target());
    let body = Body::new(Num::ONE).unwrap();
    let entity = volley.sim.entity(target_id);
    volley.sim.insert(target_id, body);
    volley.attack(shooter_id, target_id);

    // The attack starts in tick 0 and fires in tick 2, from the origin. The projectile, of no
    // width, flies from tick 3, half a meter a tick: the target's body of 1 m at 5 m starts at
    // 4 m, 8 steps, ticks 3 to 10, and it strikes in the last, which it ends on. The next attack
    // starts in tick 10 and fires in tick 12.
    let mut flights = Vec::new();
    let mut healths = Vec::new();
    for _ in 0..=12 {
        volley.sim.step();
        flights.push(volley.projectiles());
        healths.push(volley.sim.health(target_id));
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
    let attackers = volley.sim.world.get::<RecentAttackers>(entity).unwrap();
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
    assert_eq!(volley.rolls(), [roll]);
}

#[test]
fn a_projectile_whose_target_dies_or_goes_first_ends_without_a_hit() {
    let mut volley = Volley::new();
    let first = volley.unit(0, at(0, 0), shooter(volley.bolt));
    let second = volley.unit(0, at(0, 1), shooter(volley.bolt));
    let doomed = volley.unit(1, at(5, 0), target());
    let gone = volley.unit(1, at(-5, 1), target());
    volley.attack(first, doomed);
    volley.attack(second, gone);

    // Both fire in tick 2. Before tick 5 one target dies and the other despawns: in tick 5 both
    // projectiles end where they were, and neither strikes.
    for _ in 0..5 {
        volley.sim.step();
    }
    assert_eq!(volley.projectiles().len(), 2);
    volley.sim.insert(doomed, Dead);
    let entity = volley.sim.entity(gone);
    volley.sim.world.despawn(entity);

    // A projectile is state while it flies, and so are the units a cast struck.
    let hits = [first, second].map(|by| Struck { by, unit: doomed });
    let bytes = postcard::to_allocvec(&hits).unwrap();
    let struck = postcard::from_bytes::<StruckUnits>(&bytes).unwrap();
    volley.sim.world.insert_resource(struck);
    // A restore loads the match's books first: the same weapons, in the same order.
    let mut restored = Volley::new();
    for armed in [
        shooter(restored.bolt),
        shooter(restored.bolt),
        target(),
        target(),
    ] {
        let _weapon = armed.bundle(&mut restored.sim.world, Team::new(0));
    }
    volley.sim.restore_into(&mut restored.sim);
    assert_eq!(restored.projectiles(), volley.projectiles());
    let struck = |volley: &Volley| volley.sim.world.resource::<StruckUnits>().clone();
    assert_eq!(struck(&restored), struck(&volley));
    // The struck units decode only in order, each once: the ids were allocated in order.
    let decode = |hits: &[(StableId, StableId)]| {
        let hits: Vec<Struck> = hits.iter().map(|&(by, unit)| Struck { by, unit }).collect();
        let bytes = postcard::to_allocvec(&hits).unwrap();
        postcard::from_bytes::<StruckUnits>(&bytes)
    };
    assert!(decode(&[(first, doomed), (first, gone), (second, doomed)]).is_ok());
    assert!(decode(&[(first, gone), (first, doomed)]).is_err());
    assert!(decode(&[(first, doomed), (first, doomed)]).is_err());

    volley.sim.step();
    assert_eq!(volley.sim.now().get(), 6);
    assert_eq!(volley.projectiles(), []);
    assert_eq!(volley.sim.health(doomed), 100);
}

#[test]
fn a_projectile_that_outlives_its_source_kills_with_no_killer() {
    // Two shooters fire at a target of 60 health in tick 2, from 5 m either side: both projectiles
    // strike in tick 12, 30 damage each, the lower id's first. The first shooter despawns in tick
    // 5, as a dead creep does, so its strike has no source that exists: the second one's strike
    // kills, and it alone is the killer's. Were the second shooter gone too, no one would be.
    for second_goes in [false, true] {
        let mut volley = Volley::new();
        let shooters = [0, 10].map(|x| volley.unit(0, at(x, 0), shooter(volley.bolt)));
        let victim = volley.unit(1, at(5, 0), target());
        let victim_entity = volley.sim.entity(victim);
        let mut pools = volley.sim.world.get_mut::<Pools>(victim_entity).unwrap();
        pools.take(PoolId::FIRST, Num::int(40));
        for shooter in shooters {
            volley.attack(shooter, victim);
        }
        for tick in 0..=12 {
            if tick == 5 {
                let gone: &[StableId] = if second_goes {
                    &shooters
                } else {
                    &shooters[..1]
                };
                for &id in gone {
                    let entity = volley.sim.entity(id);
                    volley.sim.world.despawn(entity);
                }
            }
            volley.sim.step();
        }
        assert_eq!(volley.sim.health(victim), 0);
        let deaths: Vec<_> = volley
            .sim
            .world
            .resource::<Deaths>()
            .iter()
            .map(|death| (death.fallen.unit, death.killer, death.assisters.to_vec()))
            .collect();
        let killer = (!second_goes).then_some(shooters[1]);
        assert_eq!(
            deaths,
            [(victim, killer, vec![])],
            "second goes: {second_goes}"
        );
        // Only a source that exists is an attacker.
        let attackers: Vec<_> = volley
            .sim
            .world
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
    let mut volley = Volley::new();
    let source = volley.unit(0, at(0, -9), target());
    let point = |x: Num, z: Num| Position::new(Vec3::new(x, Num::ZERO, z)).unwrap();
    let e = Num::EPSILON;
    // The lance's reach is half its width, 0.5 m, and these bodies have no radius. Its path runs
    // along x from 0 to 6: 0.5 m beside it and 0.5 m past its end are in reach, one bit more is
    // not, and an ally is never hit.
    let beside = volley.unit(1, point(Num::int(2), Num::HALF), target());
    let wide = volley.unit(1, point(Num::int(3), Num::HALF + e), target());
    let ally = volley.unit(0, at(1, 0), target());
    let past = volley.unit(1, point(Num::int(6) + Num::HALF, Num::ZERO), target());
    let beyond = volley.unit(1, point(Num::int(6) + Num::HALF + e, Num::ZERO), target());
    volley.fire_line(source, volley.lance);

    // It launches in tick 0 and flies half a meter a tick from tick 1: 6 m is ticks 1 to 12, and
    // it ends in tick 12, as its range runs out. After tick 11 it is at 5.5 m.
    for _ in 0..12 {
        volley.sim.step();
    }
    assert_eq!(
        volley.projectiles(),
        [point(Num::int(5) + Num::HALF, Num::ZERO)]
    );
    volley.sim.step();
    assert_eq!(volley.projectiles(), []);
    let healths = [beside, wide, ally, past, beyond].map(|unit| volley.sim.health(unit));
    assert_eq!(healths, [90, 100, 100, 90, 100]);

    // A dart of no width ends at its first hit: of two enemies on one point, the lower id, in
    // tick 6, where its path reaches 3 m from 2.5 m; the one past them is never hit.
    let mut volley = Volley::new();
    let source = volley.unit(0, at(0, -9), target());
    let [first, second, behind] =
        [at(3, 0), at(3, 0), at(4, 0)].map(|at| volley.unit(1, at, target()));
    volley.fire_line(source, volley.dart);
    for _ in 0..6 {
        volley.sim.step();
    }
    assert_eq!(
        volley.projectiles(),
        [point(Num::int(2) + Num::HALF, Num::ZERO)]
    );
    volley.sim.step();
    assert_eq!(volley.projectiles(), []);
    let healths = [first, second, behind].map(|unit| volley.sim.health(unit));
    assert_eq!(healths, [90, 100, 100]);
}

#[test]
fn a_projectile_reads_only_with_a_positive_speed_and_no_negative_width_or_range() {
    let read = |text: &str| toml::from_str::<ProjectileData>(text);
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    let plain = ProjectileData::flying(Num::HALF);
    assert_eq!(read("speed = \"0.5\"").unwrap(), plain);
    assert_eq!(
        read("speed = 20\nwidth = 0\nrange = 0").unwrap(),
        ProjectileData {
            speed: Num::int(20),
            range: Some(Num::ZERO),
            ..plain
        }
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
