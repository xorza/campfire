use bevy_ecs::component::Component;
use bevy_ecs::system::RunSystemOnce;
use campfire_math::{Num, SegmentSeed, Vec3};
use campfire_sim::{IdAllocator, SimUpdate, TypeHash};

use super::*;
use crate::combat::combatant::Combatant;
use crate::combat::recent_attackers::RecentAttack;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks.
fn combatant(health: i64, range: i64, windup: u32, period: u32, damage: i64) -> Combatant {
    Combatant {
        health: Health::new(num(health)).unwrap(),
        attack: AttackStats::new(num(range), windup, period, num(damage)).unwrap(),
        on_death: OnDeath::Despawn,
    }
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
fn fighter() -> Combatant {
    combatant(100, 2, 2, 5, 30)
}

/// 100 health; never attacks.
fn dummy() -> Combatant {
    combatant(100, 0, 0, 1, 0)
}

#[derive(Debug)]
struct Fight {
    world: World,
    registry: StateRegistry,
}

impl Fight {
    fn new() -> Fight {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        Combat::install(&mut world, &mut schedule, &mut registry);
        world.add_schedule(schedule);
        Fight { world, registry }
    }

    fn unit(&mut self, team: Team, at: Position, combatant: Combatant) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world.spawn((id, at, combatant.bundle(team)));
        id
    }

    fn get<C: Component + Copy>(&self, id: StableId) -> Option<C> {
        self.get_ref(id).copied()
    }

    fn get_ref<C: Component>(&self, id: StableId) -> Option<&C> {
        let entity = self.world.resource::<EntityIndex>().get(id)?;
        self.world.entity(entity).get::<C>()
    }

    fn attack(&mut self, attacker: StableId, target: StableId) {
        let entity = self.world.resource::<EntityIndex>().get(attacker).unwrap();
        self.world
            .get_mut::<AttackState>(entity)
            .unwrap()
            .set_target(Some(target));
    }

    fn run_until(&mut self, tick: u64) {
        while self.world.resource::<SimTick>().get() < tick {
            self.world.run_schedule(SimUpdate);
        }
    }

    /// `None` once the unit despawned.
    fn health(&self, id: StableId) -> Option<i64> {
        self.get::<Health>(id)
            .map(|health| health.current().round())
    }

    fn state(&self, id: StableId) -> AttackState {
        self.get::<AttackState>(id).unwrap()
    }
}

#[test]
fn an_attack_winds_up_and_strikes_each_period() {
    let mut fight = Fight::new();
    let fighter = fight.unit(Team::new(0), at(4, 0, 0), fighter());
    let dummy = fight.unit(Team::new(1), at(6, 0, 0), dummy());
    fight.attack(fighter, dummy);

    // In range from tick 0: attacks start in ticks 0, 5, 10 and 15, and strike 2 ticks later,
    // taking 100 to 70, 40, 10 and 0: the dummy despawns in tick 17.
    fight.run_until(2);
    assert_eq!(fight.state(fighter).started(), Some(0));
    assert_eq!(fight.health(dummy), Some(100));
    fight.run_until(3);
    assert_eq!(fight.health(dummy), Some(70));
    assert_eq!(fight.state(fighter).ready_at(), 5);
    assert_eq!(fight.state(fighter).started(), None);
    let attackers = |fight: &Fight| {
        let attackers = fight.get_ref::<RecentAttackers>(dummy).unwrap();
        attackers.iter().collect::<Vec<_>>()
    };
    let attack = |source, tick| RecentAttack { source, tick };
    assert_eq!(attackers(&fight), [attack(fighter, 2)]);
    fight.run_until(8);
    assert_eq!(attackers(&fight), [attack(fighter, 7)]);
    fight.run_until(17);
    assert_eq!(fight.health(dummy), Some(10));
    fight.run_until(18);
    assert_eq!(fight.health(dummy), None);
    assert_eq!(fight.state(fighter).target(), Some(dummy));
    // The next tick finds the target gone and drops it.
    fight.run_until(19);
    assert_eq!(fight.state(fighter).target(), None);

    // A list keeps each attacker once, by stable id, and forgets one that despawned: the dummy.
    let index = fight.world.resource::<EntityIndex>();
    let mut recent = RecentAttackers::default();
    recent.record(dummy, 3, index);
    recent.record(fighter, 4, index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 4)]);
    recent.record(fighter, 6, index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 6)]);
}

#[test]
fn a_windup_on_a_target_that_dies_spends_nothing() {
    let mut fight = Fight::new();
    let slow = fight.unit(Team::new(0), at(0, 0, 0), fighter());
    let quick = fight.unit(Team::new(0), at(0, 0, 1), combatant(100, 2, 1, 5, 30));
    let prey = fight.unit(Team::new(1), at(1, 0, 0), combatant(30, 0, 0, 1, 0));
    fight.attack(slow, prey);
    fight.attack(quick, prey);

    // Both start in tick 0; the quick one strikes in tick 1 and kills the prey, before the slow
    // one's strike in tick 2. The slow one drops its target in tick 2 and is ready at once.
    fight.run_until(2);
    assert_eq!(fight.health(prey), None);
    assert_eq!(fight.state(slow).started(), Some(0));
    fight.run_until(3);
    assert_eq!(fight.state(slow), AttackState::default());
    assert_eq!(fight.state(quick).ready_at(), 5);
}

#[test]
fn strikes_in_one_tick_see_the_state_before_any_of_them() {
    let mut fight = Fight::new();
    let duelist = Combatant {
        on_death: OnDeath::Stay,
        ..combatant(30, 2, 1, 5, 30)
    };
    let first = fight.unit(Team::new(0), at(0, 0, 0), duelist);
    let second = fight.unit(Team::new(1), at(1, 0, 0), duelist);
    fight.attack(first, second);
    fight.attack(second, first);

    // Both attacks start in tick 0 and strike in tick 1: each kills the other, and both stay,
    // each with the other as its attacker.
    fight.run_until(2);
    for (unit, other) in [(first, second), (second, first)] {
        assert_eq!(fight.health(unit), Some(0));
        assert!(fight.get::<Dead>(unit).is_some());
        assert_eq!(fight.state(unit).target(), None);
        let attackers = fight.get_ref::<RecentAttackers>(unit).unwrap();
        let attack = RecentAttack {
            source: other,
            tick: 1,
        };
        assert_eq!(attackers.iter().collect::<Vec<_>>(), [attack]);
    }

    // A dead unit is no target.
    let third = fight.unit(Team::new(1), at(0, 0, 1), fighter());
    fight.attack(third, first);
    fight.run_until(3);
    assert_eq!(fight.state(third).target(), None);
}

#[test]
fn targets_are_living_enemies() {
    let mut fight = Fight::new();
    let prey = combatant(10, 0, 0, 1, 0);
    // Up at y = 9: a target all the same.
    let high = fight.unit(Team::new(0), at(0, 9, 3), prey);
    let far = fight.unit(Team::new(0), at(6, 0, 0), prey);
    let dead = fight.unit(
        Team::new(0),
        at(0, 0, 1),
        Combatant {
            on_death: OnDeath::Stay,
            ..prey
        },
    );
    let entity = fight.world.resource::<EntityIndex>().get(dead).unwrap();
    fight.world.entity_mut(entity).insert(Dead);

    let enemy_at = |fight: &mut Fight, team: Team, target: StableId| {
        fight
            .world
            .run_system_once(move |targets: Targets<'_, '_>| targets.enemy_at(team, target))
            .unwrap()
    };
    assert_eq!(enemy_at(&mut fight, Team::new(1), far), Some(at(6, 0, 0)));
    assert_eq!(enemy_at(&mut fight, Team::new(0), far), None);
    assert_eq!(enemy_at(&mut fight, Team::new(1), dead), None);
    assert_eq!(enemy_at(&mut fight, Team::new(1), high), Some(at(0, 9, 3)));
    let entity = fight.world.resource::<EntityIndex>().get(high).unwrap();
    fight.world.despawn(entity);
    assert_eq!(enemy_at(&mut fight, Team::new(1), high), None);
}

#[test]
fn teams_are_enemies_unless_the_same() {
    // A neutral team is an index like any other: 200 below is an enemy of both sides.
    for (a, b, enemies) in [
        (Team::new(0), Team::new(1), true),
        (Team::new(0), Team::new(200), true),
        (Team::new(1), Team::new(200), true),
        (Team::new(0), Team::new(0), false),
        (Team::new(200), Team::new(200), false),
    ] {
        assert_eq!(a.is_enemy_of(b), enemies, "{a:?} {b:?}");
        assert_eq!(b.is_enemy_of(a), enemies, "{b:?} {a:?}");
    }
}

#[test]
fn every_combat_type_is_state_and_restores() {
    let mut fight = Fight::new();
    let fighter = fight.unit(Team::new(0), at(0, 0, 0), fighter());
    let doomed = fight.unit(
        Team::new(1),
        at(1, 0, 0),
        Combatant {
            on_death: OnDeath::Stay,
            ..combatant(30, 0, 0, 1, 0)
        },
    );
    fight.attack(fighter, doomed);
    fight.run_until(3);
    assert!(fight.get::<Dead>(doomed).is_some());

    let registry = &fight.registry;
    let mut per_type = Vec::new();
    let hash = registry.hash_by_type(&fight.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    assert_eq!(
        names,
        [
            "combat.attack",
            "combat.attack_stats",
            "combat.dead",
            "combat.health",
            "combat.on_death",
            "combat.recent_attackers",
            "combat.team",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
        ]
    );

    let mut snapshot = Vec::new();
    registry.snapshot(&fight.world, &mut snapshot);
    let mut restored = Fight::new();
    registry.restore(&snapshot, &mut restored.world).unwrap();
    assert_eq!(registry.hash(&restored.world), hash);
    assert_eq!(restored.state(fighter), fight.state(fighter));
}

#[test]
fn stats_out_of_their_limits_are_refused() {
    assert_eq!(Health::new(Num::ZERO), None);
    assert_eq!(
        Health::new(Num::EPSILON).map(Health::current),
        Some(Num::EPSILON)
    );
    assert_eq!(AttackStats::new(-Num::EPSILON, 0, 1, Num::ZERO), None);
    assert_eq!(AttackStats::new(Num::ZERO, 0, 1, -Num::EPSILON), None);
    assert_eq!(AttackStats::new(Num::ZERO, 1, 1, Num::ZERO), None);
    assert!(AttackStats::new(Num::ZERO, 0, 1, Num::ZERO).is_some());
    let melee = AttackStats::new(Num::ONE, 1, 2, Num::ONE).unwrap();
    assert_eq!(melee.projectile_speed(), None);
    assert_eq!(melee.ranged(Num::ZERO), None);
    assert_eq!(melee.ranged(-Num::EPSILON), None);
    assert_eq!(
        melee
            .ranged(Num::EPSILON)
            .map(AttackStats::projectile_speed),
        Some(Some(Num::EPSILON))
    );

    // A snapshot's values pass the same limits.
    let health = |current: i64, max: i64| {
        let bytes = postcard::to_allocvec(&(num(current), num(max))).unwrap();
        postcard::from_bytes::<Health>(&bytes).ok()
    };
    assert!(health(0, 1).is_some() && health(1, 1).is_some());
    for (current, max) in [(-1, 1), (2, 1), (0, 0)] {
        assert_eq!(health(current, max), None, "{current} of {max}");
    }
    let stats = |windup: u32, period: u32, speed: Option<Num>| {
        let fields = (Num::ONE, windup, period, Num::ONE, speed);
        postcard::from_bytes::<AttackStats>(&postcard::to_allocvec(&fields).unwrap()).ok()
    };
    assert_eq!(stats(1, 2, None), Some(melee));
    assert_eq!(stats(1, 2, Some(Num::ONE)), melee.ranged(Num::ONE));
    assert_eq!(stats(2, 2, None), None);
    assert_eq!(stats(1, 2, Some(Num::ZERO)), None);
}
