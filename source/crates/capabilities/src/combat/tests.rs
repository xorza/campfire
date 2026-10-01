use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use bevy_ecs::system::RunSystemOnce;
use std::collections::BTreeMap;

use campfire_math::{PlayerSlot, RngSource, SegmentSeed, Vec3};
use campfire_sim::{Capability, IdAllocator, SimUpdate, TickRate, Ticks, TypeHash};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::combat::combat_rules::{CombatRules, Leech};
use crate::combat::combatant::Combatant;
use crate::combat::combatant::internals::Armed;
use crate::combat::damage_kind::DamageKind;
use crate::stats::Stats;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifiers::{Application, Instance};
use crate::stats::pool_book::PoolBook;
use crate::stats::stat::Stat;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_rule::StatRule;
use crate::units::relations::Relations;
use crate::values::attitude::Attitude;
use crate::values::metric::Metric;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks.
fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Armed {
    let combatant = Combatant {
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
    };
    Armed {
        combatant,
        life: num(health),
    }
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
fn fighter() -> Armed {
    combatant(100, 2, 2, 5, 30)
}

/// 100 health; never attacks.
fn dummy() -> Armed {
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
        } = TestMatch::new(&[Capability::Stats, Capability::Combat], RATE, None);
        world.add_schedule(schedule);
        Fight { world, registry }
    }

    fn unit(&mut self, team: Team, at: Position, combatant: Armed) -> StableId {
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
        self.get::<Pools>(id)
            .map(|pools| pools.current(PoolId::FIRST).unwrap().round())
    }

    fn state(&self, id: StableId) -> AttackState {
        self.get::<AttackState>(id).unwrap()
    }

    /// Gives unit `id` tags that block `blocks`, as its modifiers would.
    fn set_blocks(&mut self, id: StableId, blocks: &[Block]) {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world
            .entity_mut(entity)
            .insert(UnitTags::blocking(blocks));
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
fn a_range_counts_from_the_edge_of_each_body_in_the_maps_metric() {
    // A fighter of range 2 at x = 0, and a dummy 3 m off: out of range from center to center, in
    // range once each has a body of 0.5 m, as 3 ≤ 2 + 0.5 + 0.5; a bit farther, out again. Up at
    // y = 4, the dummy is still 3 m off on a planar map, and 5 m off on a spatial one.
    let half = Num::from_bits(1 << 23);
    for (dummy_x, dummy_y, bodies, metric, starts) in [
        (num(3), 0, false, Metric::Planar, false),
        (num(3), 0, true, Metric::Planar, true),
        (num(3) + Num::EPSILON, 0, true, Metric::Planar, false),
        (num(3), 4, true, Metric::Planar, true),
        (num(3), 0, true, Metric::Spatial, true),
        (num(3), 4, true, Metric::Spatial, false),
    ] {
        let mut fight = Fight::new();
        fight.world.insert_resource(metric);
        let fighter = fight.unit(Team::new(0), at(0, 0, 0), fighter());
        let dummy_at = Position::new(Vec3::new(dummy_x, num(dummy_y), Num::ZERO)).unwrap();
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
        assert_eq!(
            started.is_some(),
            starts,
            "{dummy_x:?} {dummy_y} {bodies} {metric:?}"
        );
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
    let stun = move |tick: Res<'_, SimTick>, mut tags: Query<'_, '_, &mut UnitTags>| {
        if tick.start() == Tick::new(2) {
            *tags.get_mut(late_entity).unwrap() =
                UnitTags::blocking(&[Block::Move, Block::Attack, Block::Cast, Block::Use]);
        }
    };
    fight.world.schedule_scope(SimUpdate, |_, schedule| {
        schedule.add_systems(stun.in_set(SimSet::Move));
    });
    fight.set_blocks(late, &[]);
    fight.run_until(1);
    fight.set_blocks(early, &[Block::Attack]);
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
    fight.set_blocks(early, &[]);
    fight.set_blocks(late, &[]);
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
    let duelist = combatant(30, 2, 1, 5, 30).on_death(OnDeath::Stay);
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
    let dead = fight.unit(Team::new(0), at(0, 0, 1), prey.on_death(OnDeath::Stay));
    let entity = fight.world.resource::<EntityIndex>().get(dead).unwrap();
    fight.world.entity_mut(entity).insert(Dead);
    // Tags that block being a target: no target; tags that block all else: a target all the same.
    let cases = [
        (&[Block::Target][..], false),
        (&[Block::Target, Block::Damage][..], false),
        (
            &[Block::Move, Block::Attack, Block::Cast, Block::Use][..],
            true,
        ),
    ];
    let units = [0, 1, 2].map(|x| {
        let unit = fight.unit(Team::new(0), at(x, 0, 5), prey);
        fight.set_blocks(unit, cases[usize::try_from(x).unwrap()].0);
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
    for (x, (unit, (blocks, target))) in (0..).zip(units.into_iter().zip(cases)) {
        let pos = target.then_some(at(x, 0, 5));
        assert_eq!(enemy_at(&mut fight, Team::new(1), unit), pos, "{blocks:?}");
    }
    // By the relations: team 2, friendly to team 0, may not attack its units; team 3, neutral,
    // may; team 200, past the 64 teams of before and hostile as every pair not set is, may.
    let mut relations = fight.world.resource_mut::<Relations>();
    relations.set(Team::new(2), Team::new(0), Attitude::Friendly, true);
    relations.set(Team::new(3), Team::new(0), Attitude::Neutral, true);
    let related = [2, 3, 200].map(|team| enemy_at(&mut fight, Team::new(team), far));
    assert_eq!(related, [None, Some(at(6, 0, 0)), Some(at(6, 0, 0))]);
    let entity = fight.world.resource::<EntityIndex>().get(high).unwrap();
    fight.world.despawn(entity);
    assert_eq!(enemy_at(&mut fight, Team::new(1), high), None);
}

#[test]
fn every_combat_type_is_state_and_restores() {
    let mut fight = Fight::new();
    let fighter = fight.unit(Team::new(0), at(0, 0, 0), fighter());
    let doomed = fight.unit(
        Team::new(1),
        at(1, 0, 0),
        combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay),
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
            "combat.on_death",
            "combat.recent_attackers",
            "combat.respawn",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
            "stats.level",
            "stats.modifiers",
            "stats.player_modifiers",
            "stats.pools",
            "units.body",
            "units.owner",
            "units.relations",
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
    let life = |max| Pools::new([(PoolId::FIRST, max)]);
    assert_eq!(life(Num::ZERO), None);
    assert_eq!(
        life(Num::EPSILON).and_then(|pools| pools.current(PoolId::FIRST)),
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
        let mut meters = [None; Pools::LIMIT];
        meters[0] = Some((num(current), num(max), 0_u32));
        let bytes = postcard::to_allocvec(&meters).unwrap();
        postcard::from_bytes::<Pools>(&bytes).ok()
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
        let stays = combatant(130, 0, 0, 1, 0).on_death(OnDeath::Stay);
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
    let stays = combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay);
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

/// Loads a book of the stats `[combat]` binds, with no limits, and binds them: the heal scale
/// `healing_received_pct`, and leech `life_steal` from attacks and `spell_vamp` from the rest,
/// in that order among a unit's values.
fn load_damage_stats(world: &mut World) {
    let [heal, attack, other] =
        ["healing_received_pct", "life_steal", "spell_vamp"].map(|name| Stat::named(name).unwrap());
    let rules: BTreeMap<_, _> = [&heal, &attack, &other]
        .map(|stat| (stat.clone(), StatRule::default()))
        .into();
    let book = StatBook::new(&rules, [], RATE, num(10)).unwrap();
    let combat = CombatRules {
        leech: Some(Leech {
            attack: Some(attack),
            other: Some(other),
        }),
        heal_scale: Some(heal),
        ..CombatRules::default()
    };
    world.insert_resource(CombatBindings::new(&combat, &BTreeMap::new(), &book));
    Stats::load(world, book, PoolBook::default());
}

/// A modifier of no stats, tags or script, as each shield the shield test gives is.
fn shield_data() -> ModifierData {
    ModifierData {
        script: None,
        duration_ms: None,
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        tags: Vec::new(),
        shield: None,
        aura: None,
        affects: None,
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    }
}

impl Fight {
    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    /// Gives `id` the values of `load_damage_stats`' stats.
    fn stats(&mut self, id: StableId, values: [Num; 3]) {
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
        self.get::<Pools>(id)
            .unwrap()
            .current(PoolId::FIRST)
            .unwrap()
    }
}

const ATTACK: DamageCause = DamageCause::Attack { roll: Num::ZERO };

#[test]
fn the_pass_deals_damage_in_its_order_and_credits_the_kill() {
    let mut fight = Fight::new();
    fight.world.insert_resource(AssistWindow(Ticks::new(10)));
    let a = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    let b = fight.unit(Team::new(0), at(1, 0, 0), dummy());
    let stays = dummy().on_death(OnDeath::Stay);
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
    fight.set_blocks(guarded, &[Block::Target, Block::Damage]);
    fight.set_blocks(hidden, &[Block::Target]);
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
    load_damage_stats(&mut fight.world);
    let half = Num::ONE / 2;
    let source = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    let target = fight.unit(Team::new(1), at(1, 0, 0), dummy());
    // Heals halved, life steal 0.5 and spell vamp 0.25; at 40 of 100.
    fight.stats(source, [-half, half, Num::ONE / 4]);
    let entity = fight.entity(source);
    let mut pools = fight.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, num(60));
    let shield = |id: u16, until: Option<u64>, amount: i64| Instance {
        id: ModifierId::new(id),
        source: None,
        ability: None,
        rank: 1,
        passive: false,
        held: false,
        aura_radius: None,
        stacks: 1,
        until: until.map(Tick::new),
        stack_life: None,
        stack_ends: Vec::new(),
        interval: None,
        shield: Some(num(amount)),
        stats: Vec::new(),
        tags: TagSet::default(),
        state: Vec::new(),
    };
    for name in ["first", "second", "third"] {
        Stats::load_modifier(&mut fight.world, 0, name, &shield_data(), None);
    }
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
    // Without the bindings the same stats do nothing: at 50, an attack of 10 heals the source
    // nothing, and a heal of 10 is whole.
    fight.world.insert_resource(CombatBindings::default());
    let entity = fight.entity(source);
    let mut pools = fight.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, num(50));
    fight.damage(Some(source), target, 10, ATTACK);
    fight.run_until(3);
    assert_eq!(fight.exact_health(target), num(45));
    assert_eq!(fight.exact_health(source), num(50));
    Combat::heal(&mut fight.world, source, num(10));
    assert_eq!(fight.exact_health(source), num(60));
    // A restore reaches the pool it names, unscaled: a second pool at 50 of 100 takes 20 more,
    // and the life pool keeps its 60.
    let mana = PoolId::new(1).unwrap();
    let entity = fight.entity(source);
    let mut pools = fight.world.get_mut::<Pools>(entity).unwrap();
    *pools = Pools::new([(PoolId::FIRST, num(100)), (mana, num(100))]).unwrap();
    pools.take(PoolId::FIRST, num(40));
    pools.take(mana, num(50));
    Combat::restore(&mut fight.world, source, mana, num(20));
    let pools = fight.get::<Pools>(source).unwrap();
    assert_eq!(
        (pools.current(PoolId::FIRST), pools.current(mana)),
        (Some(num(60)), Some(num(70)))
    );
}

#[test]
fn an_attack_draws_its_roll_once_as_its_windup_ends_from_the_seed() {
    let mut fight = Fight::new();
    let dummy = fight.unit(Team::new(1), at(0, 0, 0), dummy());
    let attackers: Vec<_> = (0..16)
        .map(|_| fight.unit(Team::new(0), at(1, 0, 0), combatant(100, 2, 0, 5, 30)))
        .collect();
    // Each attacker's windup ends in tick 0; the roll of each, by the attackers' order.
    let rolls = |fight: &mut Fight| {
        for &attacker in &attackers {
            let entity = fight.entity(attacker);
            let mut attack = fight.world.get_mut::<AttackState>(entity).unwrap();
            attack.set_target(Some(dummy));
            attack.start(Tick::new(0));
        }
        fight.world.resource_mut::<DamageQueue>().clear();
        fight.world.run_system_once(strike).unwrap();
        let queue = fight.world.resource::<DamageQueue>();
        let mut rolls: Vec<_> = (0..attackers.len())
            .map(|at| {
                let damage = queue.get(at).unwrap();
                (damage.source.unwrap(), damage.cause.roll().unwrap())
            })
            .collect();
        rolls.sort_unstable();
        rolls.into_iter().map(|(_, roll)| roll).collect::<Vec<_>>()
    };
    // Each roll is the first draw of its attacker's roll stream in tick 0: one draw an attack.
    let source = |seed: u8| RngSource::new(SegmentSeed::new([seed; 32]));
    let expected = |seed: u8| {
        let source = source(seed);
        let roll = |&id: &StableId| source.open(ROLL_STREAM, id.get()).fraction();
        attackers.iter().map(roll).collect::<Vec<_>>()
    };
    let first = rolls(&mut fight);
    assert_eq!(first, expected(0));
    assert!(
        first
            .iter()
            .all(|&roll| Num::ZERO <= roll && roll < Num::ONE)
    );
    // A crit below a chance of one half is the crit the engine rolled from the same stream and
    // seed before scripts decided it: `chance` on the same draw.
    let half = Num::ONE / 2;
    let crits: Vec<_> = first.iter().map(|&roll| roll < half).collect();
    let old = source(0);
    let rolled = |&id: &StableId| old.open(ROLL_STREAM, id.get()).chance(half);
    assert_eq!(crits, attackers.iter().map(rolled).collect::<Vec<_>>());
    assert!(crits.contains(&true) && crits.contains(&false));
    fight
        .world
        .insert_resource(SimRng::new(SegmentSeed::new([1; 32])));
    let second = rolls(&mut fight);
    assert_eq!(second, expected(1));
    assert_ne!(first, second);
}
