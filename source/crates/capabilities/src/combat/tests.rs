use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use bevy_ecs::system::RunSystemOnce;
use std::collections::BTreeMap;

use campfire_math::{PlayerSlot, RngSource, SegmentSeed, Vec3};
use campfire_sim::{Capability, IdAllocator, SimUpdate, TickRate, Ticks, TypeHash};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::combat::combatant::Combatant;
use crate::combat::damage_kind::DamageKind;
use crate::stats::Stats;
use crate::stats::modifier_data::Reapply;
use crate::stats::modifiers::{Application, Instance};
use crate::stats::stat::Stat;
use crate::stats::stat_rule::{Combine, StatRule};
use crate::stats::unit_state::UnitState;
use crate::stats::unit_states::UnitStates;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks.
fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Combatant {
    Combatant {
        health: Health::new(num(health)).unwrap(),
        attack: Some(
            AttackStats::new(
                num(range),
                Ticks::new(windup),
                Ticks::new(period),
                num(damage),
            )
            .unwrap(),
        ),
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
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(&[Capability::Combat], RATE, None);
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
        while self.world.resource::<SimTick>().start().get() < tick {
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

    /// Puts unit `id` in `states`, as its modifiers would.
    fn set_states(&mut self, id: StableId, states: &[UnitState]) {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world
            .entity_mut(entity)
            .insert(UnitStats::in_states(states));
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
    assert_eq!(fight.state(fighter).started(), Some(Tick::new(0)));
    assert_eq!(fight.health(dummy), Some(100));
    fight.run_until(3);
    assert_eq!(fight.health(dummy), Some(70));
    assert_eq!(fight.state(fighter).ready_at(), Tick::new(5));
    assert_eq!(fight.state(fighter).started(), None);
    let attackers = |fight: &Fight| {
        let attackers = fight.get_ref::<RecentAttackers>(dummy).unwrap();
        attackers.iter().collect::<Vec<_>>()
    };
    let attack = |source, tick| RecentAttack {
        source,
        tick: Tick::new(tick),
    };
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
    recent.record(dummy, Tick::new(3), index);
    recent.record(fighter, Tick::new(4), index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 4)]);
    recent.record(fighter, Tick::new(6), index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 6)]);
}

#[test]
fn a_range_counts_from_the_edge_of_each_body() {
    // A fighter of range 2 at x = 0, and a dummy 3 m off: out of range from center to center, in
    // range once each has a body of 0.5 m, as 3 ≤ 2 + 0.5 + 0.5; a bit farther, out again.
    let half = Num::from_bits(1 << 23);
    for (dummy_x, bodies, starts) in [
        (num(3), false, false),
        (num(3), true, true),
        (num(3) + Num::EPSILON, true, false),
    ] {
        let mut fight = Fight::new();
        let fighter = fight.unit(Team::new(0), at(0, 0, 0), fighter());
        let dummy_at = Position::new(Vec3::new(dummy_x, Num::ZERO, Num::ZERO)).unwrap();
        let dummy = fight.unit(Team::new(1), dummy_at, dummy());
        if bodies {
            for unit in [fighter, dummy] {
                let entity = fight.world.resource::<EntityIndex>().get(unit).unwrap();
                fight
                    .world
                    .entity_mut(entity)
                    .insert(Body::new(half).unwrap());
            }
        }
        fight.attack(fighter, dummy);
        fight.run_until(1);
        let started = fight.state(fighter).started();
        assert_eq!(started.is_some(), starts, "{dummy_x:?} {bodies}");
    }
}

#[test]
fn a_windup_its_attackers_states_stop_starts_again_and_spends_nothing() {
    let mut fight = Fight::new();
    let early = fight.unit(Team::new(0), at(0, 0, 0), fighter());
    let late = fight.unit(Team::new(0), at(0, 0, 1), fighter());
    let dummy = fight.unit(Team::new(1), at(1, 0, 0), dummy());
    fight.attack(early, dummy);
    fight.attack(late, dummy);
    // Both start in tick 0, to strike in tick 2. The early one is disarmed before tick 1: Act
    // interrupts its windup and keeps its target. The late one is stunned in tick 2's Move
    // stage, after Act, so its strike in Hit is interrupted instead.
    let late_entity = fight.world.resource::<EntityIndex>().get(late).unwrap();
    let stun = move |tick: Res<'_, SimTick>, mut stats: Query<'_, '_, &mut UnitStats>| {
        if tick.start() == Tick::new(2) {
            let states = UnitStates::of([UnitState::Stunned]);
            stats.get_mut(late_entity).unwrap().set_states(states);
        }
    };
    fight.world.schedule_scope(SimUpdate, |_, schedule| {
        schedule.add_systems(stun.in_set(SimSet::Move));
    });
    fight.set_states(late, &[]);
    fight.run_until(1);
    fight.set_states(early, &[UnitState::Disarmed]);
    fight.run_until(2);
    assert_eq!(fight.state(early).started(), None);
    assert_eq!(fight.state(late).started(), Some(Tick::new(0)));
    fight.run_until(3);
    assert_eq!(fight.health(dummy), Some(100));
    for unit in [early, late] {
        let state = fight.state(unit);
        assert_eq!(
            (state.target(), state.started(), state.ready_at()),
            (Some(dummy), None, Tick::new(0))
        );
    }
    // The states end before tick 3: both start again in tick 3, ready at once as no strike spent
    // the period, and strike in tick 5, 100 − 2 × 30 = 40, ready again in tick 3 + 5 = 8.
    fight.set_states(early, &[]);
    fight.set_states(late, &[]);
    fight.run_until(4);
    assert_eq!(fight.state(early).started(), Some(Tick::new(3)));
    fight.run_until(6);
    assert_eq!(fight.health(dummy), Some(40));
    assert_eq!(fight.state(late).ready_at(), Tick::new(8));
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
    assert_eq!(fight.state(slow).started(), Some(Tick::new(0)));
    fight.run_until(3);
    assert_eq!(fight.state(slow), AttackState::default());
    assert_eq!(fight.state(quick).ready_at(), Tick::new(5));
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
            tick: Tick::new(1),
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
    // Untargetable or invulnerable: no target; stunned: a target all the same.
    let cases = [
        (UnitState::Untargetable, false),
        (UnitState::Invulnerable, false),
        (UnitState::Stunned, true),
    ];
    let units = cases.map(|(state, _)| {
        let unit = fight.unit(Team::new(0), at(state as i64, 0, 5), prey);
        fight.set_states(unit, &[state]);
        unit
    });

    let enemy_at = |fight: &mut Fight, team: Team, target: StableId| {
        fight
            .world
            .run_system_once(move |targets: Targets<'_, '_>| {
                targets.enemy(team, target).map(|unit| unit.pos)
            })
            .unwrap()
    };
    assert_eq!(enemy_at(&mut fight, Team::new(1), far), Some(at(6, 0, 0)));
    assert_eq!(enemy_at(&mut fight, Team::new(0), far), None);
    assert_eq!(enemy_at(&mut fight, Team::new(1), dead), None);
    assert_eq!(enemy_at(&mut fight, Team::new(1), high), Some(at(0, 9, 3)));
    for (unit, (state, target)) in units.into_iter().zip(cases) {
        let pos = target.then_some(at(state as i64, 0, 5));
        assert_eq!(enemy_at(&mut fight, Team::new(1), unit), pos, "{state:?}");
    }
    let entity = fight.world.resource::<EntityIndex>().get(high).unwrap();
    fight.world.despawn(entity);
    assert_eq!(enemy_at(&mut fight, Team::new(1), high), None);
}

#[test]
fn teams_are_enemies_unless_the_same() {
    // A neutral team is an index like any other: 63, the last, below is an enemy of both sides.
    for (a, b, enemies) in [
        (Team::new(0), Team::new(1), true),
        (Team::new(0), Team::new(63), true),
        (Team::new(1), Team::new(63), true),
        (Team::new(0), Team::new(0), false),
        (Team::new(63), Team::new(63), false),
    ] {
        assert_eq!(a.is_enemy_of(b), enemies, "{a:?} {b:?}");
        assert_eq!(b.is_enemy_of(a), enemies, "{b:?} {a:?}");
    }
    // A snapshot's team past the limit does not decode.
    let decode = |index: u8| postcard::from_bytes::<Team>(&[index]).ok();
    assert_eq!((decode(63), decode(64)), (Some(Team::new(63)), None));
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
            "combat.respawn",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
            "units.body",
            "units.owner",
            "units.spawn_point",
            "units.team",
            "units.unit_type",
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
    assert_eq!(
        AttackStats::new(-Num::EPSILON, Ticks::new(0), Ticks::new(1), Num::ZERO),
        None
    );
    assert_eq!(
        AttackStats::new(Num::ZERO, Ticks::new(0), Ticks::new(1), -Num::EPSILON),
        None
    );
    assert_eq!(
        AttackStats::new(Num::ZERO, Ticks::new(1), Ticks::new(1), Num::ZERO),
        None
    );
    assert!(AttackStats::new(Num::ZERO, Ticks::new(0), Ticks::new(1), Num::ZERO).is_some());
    let melee = AttackStats::new(Num::ONE, Ticks::new(1), Ticks::new(2), Num::ONE).unwrap();
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
        let bytes = postcard::to_allocvec(&(num(current), num(max), 0_u32)).unwrap();
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

#[test]
fn a_death_names_its_killer_and_assisters_and_the_dead_come_back_at_their_spawn() {
    // Victims of 130 health: a hero that stays, which the first, the killer and the slow one
    // attack, and a creep that despawns, which the creep's three attack in the same way. The
    // first and the killer strike every 5 ticks from tick 2, the slow one once, in tick 2, then
    // 20 ticks later. Strikes land by source id: in tick 2 the three take each victim to 40; in
    // tick 7 the first takes it to 10 and the killer to 0. The first struck 0 ticks before, the
    // slow one 5: with a window of 4 ticks only the first assisted, with 5 both, and with no
    // window no one.
    let slow = combatant(100, 2, 2, 20, 30);
    for (window, assisted) in [(None, 0), (Some(4), 1), (Some(5), 2)] {
        let mut fight = Fight::new();
        if let Some(ticks) = window {
            fight.world.insert_resource(AssistWindow(Ticks::new(ticks)));
        }
        let team = Team::new(0);
        let [first, killer] = [4, 6].map(|x| fight.unit(team, at(x, 0, 0), fighter()));
        let slow_one = fight.unit(team, at(5, 0, 0), slow);
        let stays = Combatant {
            on_death: OnDeath::Stay,
            ..combatant(130, 0, 0, 1, 0)
        };
        let hero = fight.unit(Team::new(1), at(5, 0, 1), stays);
        let hero_entity = fight.world.resource::<EntityIndex>().get(hero).unwrap();
        let owner = Owner::new(PlayerSlot::new(3));
        fight.world.entity_mut(hero_entity).insert(owner);
        let creep = fight.unit(Team::new(1), at(5, 0, -1), combatant(130, 0, 0, 1, 0));
        let [creep_first, creep_killer] = [4, 6].map(|x| fight.unit(team, at(x, 0, -1), fighter()));
        let creep_slow = fight.unit(team, at(5, 0, -1), slow);
        for (attacker, target) in [
            (first, hero),
            (killer, hero),
            (slow_one, hero),
            (creep_first, creep),
            (creep_killer, creep),
            (creep_slow, creep),
        ] {
            fight.attack(attacker, target);
        }
        fight.run_until(7);
        assert_eq!(fight.health(hero), Some(40), "{window:?}");
        fight.run_until(8);
        // Each death holds its tick, 7, and the unit as it was: its team, and its owner, a
        // record that outlives the creep.
        let deaths = fight.world.resource::<Deaths>();
        assert_eq!(deaths.tick(), Tick::new(7));
        let seen: Vec<_> = deaths
            .iter()
            .map(|death| (death.fallen, death.killer, death.assisters.to_vec()))
            .collect();
        let fallen = |unit, owner| Fallen {
            unit,
            team: Some(Team::new(1)),
            owner,
        };
        assert_eq!(
            seen,
            [
                (
                    fallen(hero, Some(PlayerSlot::new(3))),
                    Some(killer),
                    [first, slow_one][..assisted].to_vec()
                ),
                (
                    fallen(creep, None),
                    Some(creep_killer),
                    [creep_first, creep_slow][..assisted].to_vec()
                ),
            ],
            "{window:?}"
        );
        // The hero stays, dead; the creep is gone by the tick's end.
        assert!(fight.get::<Dead>(hero).is_some());
        assert_eq!(fight.health(creep), None);
    }

    // The hero comes back at the start of its respawn tick, 10, at its spawn point with full
    // health and no attacker on record; the attackers dropped it when it died.
    let mut fight = Fight::new();
    let attacker = fight.unit(Team::new(0), at(4, 0, 0), fighter());
    let stays = Combatant {
        on_death: OnDeath::Stay,
        ..combatant(30, 0, 0, 1, 0)
    };
    let hero = fight.unit(Team::new(1), at(5, 0, 0), stays);
    let hero_entity = fight.world.resource::<EntityIndex>().get(hero).unwrap();
    fight
        .world
        .entity_mut(hero_entity)
        .insert(SpawnPoint::new(at(-3, 0, 2)));
    fight.attack(attacker, hero);
    fight.run_until(3);
    assert!(fight.get::<Dead>(hero).is_some());
    let at_tick = Tick::new(10);
    fight
        .world
        .entity_mut(hero_entity)
        .insert(Respawn { at: at_tick });
    fight.run_until(10);
    assert!(fight.get::<Dead>(hero).is_some());
    fight.run_until(11);
    assert!(fight.get::<Dead>(hero).is_none() && fight.get::<Respawn>(hero).is_none());
    assert_eq!(fight.get::<Position>(hero), Some(at(-3, 0, 2)));
    assert_eq!(fight.health(hero), Some(30));
    assert_eq!(
        fight
            .get_ref::<RecentAttackers>(hero)
            .map(|r| r.iter().count()),
        Some(0)
    );
    assert_eq!(fight.state(attacker).target(), None);
}

/// A stat book of the damage system's stats, each a sum with no limits: crit chance, life
/// steal, spell vamp and healing received, in that order among a unit's values.
fn damage_stats() -> StatBook {
    let sum = StatRule {
        combine: Combine::Sum,
        min: None,
        max: None,
    };
    let rules: BTreeMap<_, _> = [
        EngineStat::CritChance,
        EngineStat::LifeSteal,
        EngineStat::SpellVamp,
        EngineStat::HealingReceivedPct,
    ]
    .map(|stat| (Stat::Engine(stat), sum))
    .into();
    StatBook::new(&rules, [], RATE, num(10)).unwrap()
}

impl Fight {
    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    /// Gives `id` the values of `damage_stats`' stats.
    fn stats(&mut self, id: StableId, values: [Num; 4]) {
        let mut stats = UnitStats::default();
        stats.refill().extend(values);
        let entity = self.entity(id);
        self.world.entity_mut(entity).insert(stats);
    }

    /// Queues `amount` of damage from `source` to `target`, dealt by `cause`.
    fn damage(
        &mut self,
        source: Option<StableId>,
        target: StableId,
        amount: i64,
        cause: DamageCause,
    ) {
        self.world.resource_mut::<DamageQueue>().push(Damage {
            source,
            target,
            amount: num(amount),
            kind: DamageKind::new(0),
            cause,
            ability: None,
            depth: 0,
        });
    }

    fn exact_health(&self, id: StableId) -> Num {
        self.get::<Health>(id).unwrap().current()
    }
}

const ATTACK: DamageCause = DamageCause::Attack { crit: false };

#[test]
fn the_pass_deals_damage_in_its_order_and_credits_the_kill() {
    let mut fight = Fight::new();
    fight.world.insert_resource(AssistWindow(Ticks::new(10)));
    let a = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    let b = fight.unit(Team::new(0), at(1, 0, 0), dummy());
    let mut stays = dummy();
    stays.on_death = OnDeath::Stay;
    let target = fight.unit(Team::new(1), at(2, 0, 0), stays);
    assert!(a < b);
    // Queued from b, then a, then none: none's 30 goes first, 100 to 70, then a's 60, to 10, then
    // b's 60, to 0. b killed it, and a, which damaged it this tick, assisted.
    fight.damage(Some(b), target, 60, ATTACK);
    fight.damage(Some(a), target, 60, DamageCause::Effect);
    fight.damage(None, target, 30, DamageCause::Effect);
    fight.run_until(1);
    assert_eq!(fight.health(target), Some(0));
    let deaths = fight.world.resource::<Deaths>();
    let died: Vec<_> = deaths
        .iter()
        .map(|death| (death.fallen.unit, death.killer, death.assisters.to_vec()))
        .collect();
    assert_eq!(died, [(target, Some(b), vec![a])]);

    // At zero health it takes nothing more, records no attacker, and heals nothing.
    let c = fight.unit(Team::new(0), at(3, 0, 0), dummy());
    fight.damage(Some(c), target, 10, ATTACK);
    fight.run_until(2);
    Combat::heal(&mut fight.world, target, num(50));
    assert_eq!(fight.health(target), Some(0));
    let attackers = fight.get_ref::<RecentAttackers>(target).unwrap();
    assert!(attackers.iter().all(|attack| attack.source != c));
    assert!(fight.world.resource::<Deaths>().is_empty());

    // An invulnerable unit takes nothing and records no attacker; untargetable, it takes all.
    let guarded = fight.unit(Team::new(1), at(4, 0, 0), dummy());
    let hidden = fight.unit(Team::new(1), at(5, 0, 0), dummy());
    fight.set_states(guarded, &[UnitState::Invulnerable]);
    fight.set_states(hidden, &[UnitState::Untargetable]);
    fight.damage(Some(c), guarded, 10, DamageCause::Effect);
    fight.damage(Some(c), hidden, 10, DamageCause::Effect);
    fight.run_until(3);
    assert_eq!(
        (fight.health(guarded), fight.health(hidden)),
        (Some(100), Some(90))
    );
    let attackers = fight.get_ref::<RecentAttackers>(guarded).unwrap();
    assert_eq!(attackers.iter().count(), 0);
}

#[test]
fn shields_absorb_soonest_end_first_and_vamps_heal_from_health_taken() {
    let mut fight = Fight::new();
    Stats::load(&mut fight.world, damage_stats());
    let half = Num::ONE / 2;
    let source = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    let target = fight.unit(Team::new(1), at(1, 0, 0), dummy());
    // Life steal 0.5, spell vamp 0.25, and heals halved; at 40 of 100.
    fight.stats(source, [Num::ZERO, half, Num::ONE / 4, -half]);
    let entity = fight.entity(source);
    fight.world.get_mut::<Health>(entity).unwrap().take(num(60));
    let shield = |id: u16, until: Option<u64>, amount: i64| Instance {
        id: ModifierId::new(id),
        source: None,
        ability: None,
        rank: 1,
        passive: false,
        aura: false,
        aura_radius: None,
        stacks: 1,
        until: until.map(Tick::new),
        stack_life: None,
        stack_ends: Vec::new(),
        interval: None,
        shield: Some(num(amount)),
        stats: Vec::new(),
        states: UnitStates::default(),
        state: Vec::new(),
    };
    let mut modifiers = Modifiers::default();
    for instance in [
        shield(0, None, 100),
        shield(1, Some(50), 20),
        shield(2, Some(30), 10),
    ] {
        modifiers.apply(Application {
            instance,
            reapply: Reapply::Refresh,
            max_stacks: None,
        });
    }
    let entity = fight.entity(target);
    fight.world.entity_mut(entity).insert(modifiers);
    let shields = |fight: &Fight| {
        let modifiers = fight.get_ref::<Modifiers>(target).unwrap();
        let shields = modifiers.iter().map(|instance| instance.shield.unwrap());
        shields.collect::<Vec<_>>()
    };

    // 35: the shield that ends in tick 30 spends its 10, the one of tick 50 its 20, and the one
    // with no end 5 of its 100; health takes nothing, and the source heals nothing.
    fight.damage(Some(source), target, 35, ATTACK);
    fight.run_until(1);
    assert_eq!(shields(&fight), [num(95)]);
    assert_eq!(fight.exact_health(target), num(100));
    assert_eq!(fight.exact_health(source), num(40));
    // An attack of 100: the last shield's 95, then 5 off health, which life steals 5 × 0.5,
    // halved: 1.25. Then a spell of 40, all off health: 40 × 0.25, halved, 5. 40 + 1.25 + 5.
    fight.damage(Some(source), target, 100, ATTACK);
    fight.damage(Some(source), target, 40, DamageCause::Effect);
    fight.run_until(2);
    assert_eq!(shields(&fight), []);
    assert_eq!(fight.exact_health(target), num(55));
    assert_eq!(fight.exact_health(source), num(185) / 4);
    // A heal of 10 is halved too; one past the maximum stops at it.
    Combat::heal(&mut fight.world, source, num(10));
    assert_eq!(fight.exact_health(source), num(205) / 4);
    Combat::heal(&mut fight.world, source, num(1000));
    assert_eq!(fight.exact_health(source), num(100));
    // A restore is not scaled: 50 of 100, then 20 more.
    let entity = fight.entity(source);
    let mut pool = ResourcePool::new(num(100)).unwrap();
    pool.spend(num(50));
    fight.world.entity_mut(entity).insert(pool);
    Combat::restore(&mut fight.world, source, num(20));
    assert_eq!(
        fight.get::<ResourcePool>(source).unwrap().current(),
        num(70)
    );
}

#[test]
fn an_attack_rolls_its_crit_once_as_its_windup_ends_from_the_seed() {
    let mut fight = Fight::new();
    Stats::load(&mut fight.world, damage_stats());
    let dummy = fight.unit(Team::new(1), at(0, 0, 0), dummy());
    let attackers: Vec<_> = (0..16)
        .map(|_| fight.unit(Team::new(0), at(1, 0, 0), combatant(100, 2, 0, 5, 30)))
        .collect();
    // Each attacker's windup ends in tick 0, with a crit chance of `chance`; the crit of each.
    let roll = |fight: &mut Fight, chance: Num| {
        for &attacker in &attackers {
            fight.stats(attacker, [chance, Num::ZERO, Num::ZERO, Num::ZERO]);
            let entity = fight.entity(attacker);
            let mut attack = fight.world.get_mut::<AttackState>(entity).unwrap();
            attack.set_target(Some(dummy));
            attack.start(Tick::new(0));
        }
        fight.world.resource_mut::<DamageQueue>().clear();
        fight.world.run_system_once(strike).unwrap();
        let queue = fight.world.resource::<DamageQueue>();
        let mut crits: Vec<_> = (0..attackers.len())
            .map(|at| {
                let damage = queue.get(at).unwrap();
                (damage.source.unwrap(), damage.cause.crit())
            })
            .collect();
        crits.sort_unstable();
        crits.into_iter().map(|(_, crit)| crit).collect::<Vec<_>>()
    };
    // Each crit is the first draw of its attacker's crit stream in tick 0: one draw an attack.
    let half = Num::ONE / 2;
    let expected = |seed: u8| {
        let source = RngSource::new(SegmentSeed::new([seed; 32]));
        let crit = |&id: &StableId| source.open(CRIT_STREAM, id.get()).chance(half);
        attackers.iter().map(crit).collect::<Vec<_>>()
    };
    let first = roll(&mut fight, half);
    assert_eq!(first, expected(0));
    assert!(first.contains(&true) && first.contains(&false));
    fight
        .world
        .insert_resource(SimRng::new(SegmentSeed::new([1; 32])));
    let second = roll(&mut fight, half);
    assert_eq!(second, expected(1));
    assert_ne!(first, second);
    // No chance never crits; a chance of 1 always does.
    fight
        .world
        .insert_resource(SimRng::new(SegmentSeed::new([0; 32])));
    assert!(roll(&mut fight, Num::ZERO).iter().all(|&crit| !crit));
    fight
        .world
        .insert_resource(SimRng::new(SegmentSeed::new([2; 32])));
    assert!(roll(&mut fight, Num::ONE).iter().all(|&crit| crit));
}
