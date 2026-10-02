use std::num::NonZeroU32;

use campfire_math::{RngSource, SegmentSeed};
use campfire_sim::{Capability, EntityIndex, IdAllocator, SimTick, SimUpdate, TypeHash};

use super::*;
use crate::actions::action_slots::ActionSlots;
use crate::capability_set::internals::TestMatch;
use crate::combat::ROLL_STREAM;
use crate::combat::armed::Armed;
use crate::combat::deaths::Deaths;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::projectiles::cast_hits::CastHit;
use crate::projectiles::projectile_data::ProjectileData;
use crate::stats::pool_id::PoolId;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::recent_attack::RecentAttack;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::damage_kind::DamageKind;
use campfire_sim::TickRate;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
}

/// Half a meter.
fn half() -> Num {
    Num::ONE.checked_div_int(2).unwrap()
}

/// A projectile type of 15 m/s, half a meter a tick, that hits enemies; homing, or along a line
/// of `width` for `range` that ends at its first hit when `stop_on_hit`.
fn projectile(homing: bool, width: Num, range: Option<Num>, stop_on_hit: bool) -> ProjectileData {
    ProjectileData {
        speed: num(15),
        width,
        range,
        homing,
        stop_on_hit,
        once_per_cast: false,
        hits: None,
        gravity: None,
        sight_radius: None,
        collide: None,
    }
}

/// 30 damage within 8 m, fired 2 ticks after the start of an attack every 10 ticks, its
/// projectiles of the homing type `bolt`.
fn shooter(bolt: UnitType) -> Armed {
    Armed::melee(num(100), num(8), 2, 10, num(30))
        .ranged(bolt)
        .on_death(OnDeath::Stay)
}

/// 100 health; never attacks; stays when it dies.
fn target() -> Armed {
    Armed::unarmed(num(100)).on_death(OnDeath::Stay)
}

#[derive(Debug)]
struct Volley {
    world: World,
    registry: StateRegistry,
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
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(&declared, RATE, None);
        world.add_schedule(schedule);
        let types = [
            ("bolt", projectile(true, Num::ZERO, None, false)),
            ("lance", projectile(false, Num::ONE, Some(num(6)), false)),
            ("dart", projectile(false, Num::ZERO, Some(num(6)), true)),
        ];
        let [bolt, lance, dart] = types.map(|(name, data)| {
            let unit_type =
                Units::load_type(&mut world, TypeScope::Mode, name, &UnitTypeData::default());
            Projectiles::load_type(&mut world, unit_type, &data);
            unit_type
        });
        Volley {
            world,
            registry,
            bolt,
            lance,
            dart,
        }
    }

    /// Launches a projectile of the line type `unit_type` from `source` at the origin along +x,
    /// carrying 10 damage, from the next tick.
    fn fire_line(&mut self, source: StableId, unit_type: UnitType) {
        let range = self
            .world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .unwrap()
            .range
            .unwrap();
        let mut launches = self.world.resource_mut::<Launches>();
        let cast = launches.cast();
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
                amount: num(10),
                kind: DamageKind::new(0),
                roll: Num::ZERO,
            },
            cast,
        });
    }

    fn unit(&mut self, team: u8, at: Position, combatant: Armed) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let bundle = combatant.bundle(&mut self.world, Team::new(team), RATE.hz().get());
        self.world.spawn((id, at, bundle));
        id
    }

    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    fn attack(&mut self, attacker: StableId, target: StableId) {
        let entity = self.entity(attacker);
        let mut slots = self.world.get_mut::<ActionSlots>(entity).unwrap();
        slots.set_attack_target(Some(target));
    }

    fn tick(&mut self) {
        self.world.run_schedule(SimUpdate);
    }

    fn health(&self, id: StableId) -> i64 {
        let entity = self.entity(id);
        let pools = self.world.get::<Pools>(entity).unwrap();
        pools.current(PoolId::FIRST).unwrap().round()
    }

    /// The roll each projectile carries, by stable id.
    fn rolls(&self) -> Vec<Num> {
        let world = &self.world;
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
        let world = &self.world;
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
    let entity = volley.entity(target_id);
    volley.world.entity_mut(entity).insert(body);
    volley.attack(shooter_id, target_id);

    // The attack starts in tick 0 and fires in tick 2, from the origin. The projectile, of no
    // width, flies from tick 3, half a meter a tick: the target's body of 1 m at 5 m starts at
    // 4 m, 8 steps, ticks 3 to 10, and it strikes in the last, which it ends on. The next attack
    // starts in tick 10 and fires in tick 12.
    let mut flights = Vec::new();
    let mut healths = Vec::new();
    for _ in 0..=12 {
        volley.tick();
        flights.push(volley.projectiles());
        healths.push(volley.health(target_id));
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
    let attackers = volley.world.get::<RecentAttackers>(entity).unwrap();
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
        volley.tick();
    }
    assert_eq!(volley.projectiles().len(), 2);
    let entity = volley.entity(doomed);
    volley.world.entity_mut(entity).insert(Dead);
    let entity = volley.entity(gone);
    volley.world.despawn(entity);

    // A projectile is state while it flies.
    let mut per_type = Vec::new();
    let hash = volley.registry.hash_by_type(&volley.world, &mut per_type);
    assert!(
        per_type
            .iter()
            .any(|TypeHash { name, .. }| *name == "projectiles.projectile")
    );
    let mut snapshot = Vec::new();
    volley.registry.snapshot(&volley.world, &mut snapshot);
    // A restore loads the match's books first: the same weapons, in the same order.
    let mut restored = Volley::new();
    let hz = RATE.hz().get();
    for armed in [
        shooter(restored.bolt),
        shooter(restored.bolt),
        target(),
        target(),
    ] {
        let _weapon = armed.bundle(&mut restored.world, Team::new(0), hz);
    }
    volley
        .registry
        .restore(&snapshot, &mut restored.world)
        .unwrap();
    assert_eq!(volley.registry.hash(&restored.world), hash);
    assert_eq!(restored.projectiles(), volley.projectiles());
    // A cast's hits decode only in order, each once: the ids were allocated in order.
    let decode = |hits: &[(StableId, StableId)]| {
        let hits: Vec<CastHit> = hits
            .iter()
            .map(|&(group, unit)| CastHit { group, unit })
            .collect();
        let bytes = postcard::to_allocvec(&hits).unwrap();
        postcard::from_bytes::<CastHits>(&bytes)
    };
    assert!(decode(&[(first, doomed), (first, gone), (second, doomed)]).is_ok());
    assert!(decode(&[(first, gone), (first, doomed)]).is_err());
    assert!(decode(&[(first, doomed), (first, doomed)]).is_err());

    volley.tick();
    assert_eq!(volley.world.resource::<SimTick>().start().get(), 6);
    assert_eq!(volley.projectiles(), []);
    assert_eq!(volley.health(doomed), 100);
}

#[test]
fn a_projectile_that_outlives_its_source_kills_with_no_killer() {
    // Two shooters fire at a target of 60 health in tick 2, from 5 m either side: both projectiles
    // strike in tick 12, 30 damage each, the lower id's first. The first shooter despawns in tick 5, as a
    // dead creep does, so its strike has no source that exists: the second one's strike kills,
    // and it alone is the killer's. Were the second shooter gone too, no one would be.
    for second_goes in [false, true] {
        let mut volley = Volley::new();
        let shooters = [0, 10].map(|x| volley.unit(0, at(x, 0), shooter(volley.bolt)));
        let victim = volley.unit(1, at(5, 0), target());
        let victim_entity = volley.entity(victim);
        let mut pools = volley.world.get_mut::<Pools>(victim_entity).unwrap();
        pools.take(PoolId::FIRST, num(40));
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
                    let entity = volley.entity(id);
                    volley.world.despawn(entity);
                }
            }
            volley.tick();
        }
        assert_eq!(volley.health(victim), 0);
        let deaths: Vec<_> = volley
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
    let beside = volley.unit(1, point(num(2), half()), target());
    let wide = volley.unit(1, point(num(3), half() + e), target());
    let ally = volley.unit(0, at(1, 0), target());
    let past = volley.unit(1, point(num(6) + half(), Num::ZERO), target());
    let beyond = volley.unit(1, point(num(6) + half() + e, Num::ZERO), target());
    volley.fire_line(source, volley.lance);

    // It launches in tick 0 and flies half a meter a tick from tick 1: 6 m is ticks 1 to 12, and
    // it ends in tick 12, as its range runs out. After tick 11 it is at 5.5 m.
    for _ in 0..12 {
        volley.tick();
    }
    assert_eq!(volley.projectiles(), [point(num(5) + half(), Num::ZERO)]);
    volley.tick();
    assert_eq!(volley.projectiles(), []);
    let healths = [beside, wide, ally, past, beyond].map(|unit| volley.health(unit));
    assert_eq!(healths, [90, 100, 100, 90, 100]);

    // A dart of no width ends at its first hit: of two enemies on one point, the lower id, in
    // tick 6, where its path reaches 3 m from 2.5 m; the one past them is never hit.
    let mut volley = Volley::new();
    let source = volley.unit(0, at(0, -9), target());
    let [first, second, behind] =
        [at(3, 0), at(3, 0), at(4, 0)].map(|at| volley.unit(1, at, target()));
    volley.fire_line(source, volley.dart);
    for _ in 0..6 {
        volley.tick();
    }
    assert_eq!(volley.projectiles(), [point(num(2) + half(), Num::ZERO)]);
    volley.tick();
    assert_eq!(volley.projectiles(), []);
    let healths = [first, second, behind].map(|unit| volley.health(unit));
    assert_eq!(healths, [90, 100, 100]);
}
