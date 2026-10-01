use std::num::NonZeroU32;

use campfire_math::{Num, RngSource, SegmentSeed, Vec3};
use campfire_sim::{
    Capability, EntityIndex, SimTick, SimUpdate, StableId, Tick, TickRate, Ticks, TypeHash,
};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::combat::ROLL_STREAM;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::combatant::internals::Armed;
use crate::combat::dead::Dead;
use crate::combat::deaths::Deaths;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::stats::pool_id::PoolId;
use crate::units::recent_attack::RecentAttack;
use crate::units::team::Team;

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

/// 30 damage within 8 m, fired 2 ticks after the start of an attack every 10 ticks, flying half
/// a meter a tick.
fn shooter() -> Armed {
    let melee = AttackStats::new(num(8), Ticks::new(2), Ticks::new(10), num(30)).unwrap();
    let combatant = Combatant {
        attack: Some(melee.ranged(half()).unwrap()),
        on_death: OnDeath::Stay,
    };
    Armed {
        combatant,
        life: num(100),
    }
}

/// 100 health; never attacks; stays when it dies.
fn target() -> Armed {
    let combatant = Combatant {
        attack: Some(AttackStats::new(Num::ZERO, Ticks::new(0), Ticks::new(1), Num::ZERO).unwrap()),
        on_death: OnDeath::Stay,
    };
    Armed {
        combatant,
        life: num(100),
    }
}

#[derive(Debug)]
struct Volley {
    world: World,
    registry: StateRegistry,
}

impl Volley {
    /// A match with combat, and with projectiles when `projectiles`.
    fn new(projectiles: bool) -> Volley {
        let declared: &[Capability] = if projectiles {
            &[
                Capability::Stats,
                Capability::Combat,
                Capability::Projectiles,
            ]
        } else {
            &[Capability::Stats, Capability::Combat]
        };
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(declared, RATE, None);
        world.add_schedule(schedule);
        Volley { world, registry }
    }

    fn unit(&mut self, team: u8, at: Position, combatant: Armed) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world
            .spawn((id, at, combatant.bundle(Team::new(team))));
        id
    }

    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    fn attack(&mut self, attacker: StableId, target: StableId) {
        let entity = self.entity(attacker);
        let mut attack = self.world.get_mut::<AttackState>(entity).unwrap();
        attack.set_target(Some(target));
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
            .map(|projectile| projectile.roll())
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
fn a_projectile_flies_to_its_target_and_strikes_on_arrival() {
    let mut volley = Volley::new(true);
    let shooter_id = volley.unit(0, at(0, 0), shooter());
    let target_id = volley.unit(1, at(5, 0), target());
    volley.attack(shooter_id, target_id);

    // The attack starts in tick 0 and fires in tick 2, from the origin. The projectile flies
    // from tick 3, half a meter a tick: 5 m is 10 steps, ticks 3 to 12, and it strikes in the
    // last, which it ends on. The next attack starts in tick 10 and fires in tick 12.
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
        .chain((0..=9).map(flying))
        .chain([flying(0)])
        .collect();
    assert_eq!(flights, expected);
    assert_eq!(healths, [[100; 12].as_slice(), &[70]].concat());
    let entity = volley.entity(target_id);
    let attackers = volley.world.get::<RecentAttackers>(entity).unwrap();
    let attack = RecentAttack {
        source: shooter_id,
        tick: Tick::new(12),
    };
    assert_eq!(attackers.iter().collect::<Vec<_>>(), [attack]);

    // The one in flight, fired in tick 12, carries the roll its attack drew as its windup
    // ended: the first draw of its attacker's roll stream in that tick, on the match's seed.
    let mut source = RngSource::new(SegmentSeed::new([0; 32]));
    source.begin_tick(12);
    let roll = source.open(ROLL_STREAM, shooter_id.get()).fraction();
    assert_eq!(volley.rolls(), [roll]);

    // Without projectiles, the same attack strikes at the end of its windup, in tick 2.
    let mut instant = Volley::new(false);
    let shooter_id = instant.unit(0, at(0, 0), shooter());
    let target_id = instant.unit(1, at(5, 0), target());
    instant.attack(shooter_id, target_id);
    let healths: Vec<_> = (0..3)
        .map(|_| {
            instant.tick();
            instant.health(target_id)
        })
        .collect();
    assert_eq!(healths, [100, 100, 70]);
}

#[test]
fn a_projectile_whose_target_dies_or_goes_first_ends_without_a_hit() {
    let mut volley = Volley::new(true);
    let first = volley.unit(0, at(0, 0), shooter());
    let second = volley.unit(0, at(0, 1), shooter());
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
    let mut restored = Volley::new(true);
    volley
        .registry
        .restore(&snapshot, &mut restored.world)
        .unwrap();
    assert_eq!(volley.registry.hash(&restored.world), hash);
    assert_eq!(restored.projectiles(), volley.projectiles());

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
        let mut volley = Volley::new(true);
        let shooters = [0, 10].map(|x| volley.unit(0, at(x, 0), shooter()));
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
