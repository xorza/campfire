use bevy_ecs::system::RunSystemOnce;
use std::collections::BTreeMap;

use campfire_common::{PlayerSlot, SegmentSeed, Ticks};
use campfire_math::{Num, RngSource, Vec3};
use campfire_sim::{EntityIndex, SimComponent};

use super::*;
use crate::actions::action_book::internals::{self, TestWeapon};
use crate::actions::range::Range;
use crate::actions::slot_kind::SlotKind;
use crate::capability_set::test_match::TestMatch;
use crate::combat::assist_window::AssistWindow;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::combat_rules::{CombatRules, Leech};
use crate::combat::internals::Armed;
use crate::combat::pass_queue::PassEntry;
use crate::combat::recent_attack::RecentAttack;
use crate::players::resource_id::ResourceId;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::Stats;
use crate::stats::application::{Application, NewInstance};
use crate::stats::lifetime::{Ends, Lifetime};
use crate::stats::modifier_data::ModifierData;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pool_data::PoolData;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_id::StatId;
use crate::stats::stat_rule::StatRule;
use crate::units::Units;
use crate::units::filter::Filter;
use crate::units::relations::Relations;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::attitude::Attitude;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::metric::Metric;
use crate::values::relation::Relation;
use crate::values::stat::Stat;
fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::int(y), Num::int(z))).unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks.
fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Armed {
    Armed::melee(
        Num::int(health),
        Num::int(range),
        windup,
        period,
        Num::int(damage),
    )
}

/// A unit's attack as these tests read it: its target, the tick the attack in its windup
/// started in, and the first tick its weapon may attack again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Attack {
    target: Option<StableId>,
    started: Option<Tick>,
    ready_at: Tick,
}

impl Attack {
    const fn target(self) -> Option<StableId> {
        self.target
    }

    const fn started(self) -> Option<Tick> {
        self.started
    }

    const fn ready_at(self) -> Tick {
        self.ready_at
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
    sim: TestMatch,
}

impl Fight {
    fn new() -> Fight {
        let budgets = ScriptBudgets::new(ScriptLimits::ROOMY, 1);
        let sim = TestMatch::server(&[Capability::Stats, Capability::Combat], budgets);
        Fight { sim }
    }

    fn unit(&mut self, team: Team, at: Position, combatant: Armed) -> StableId {
        let bundle = combatant.bundle(&mut self.sim.world, team);
        self.sim.spawn(at, bundle)
    }

    fn attack(&mut self, attacker: StableId, target: StableId) {
        let entity = self.sim.entity(attacker);
        self.sim
            .world
            .get_mut::<ActionSlots>(entity)
            .unwrap()
            .set_attack_target(Some(target));
    }

    /// The attack of unit `id`, whose one weapon sits in its first slot.
    fn state(&self, id: StableId) -> Attack {
        let slots = self.sim.try_get::<ActionSlots>(id).unwrap();
        let slot = slots.slot(0).unwrap();
        let book = self.sim.world.resource::<ActionBook>();
        let windup = book.get(slot.action).unwrap().ranks[0].windup;
        let started = slots.attacking().and(slots.in_progress());
        let started = started.and_then(InProgress::resolves_at);
        Attack {
            target: slots.attack_target(),
            started: started.map(|at| Tick::new(at.get() - windup.get())),
            ready_at: slot.ready_at,
        }
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
    fight.sim.run_until(2);
    assert_eq!(fight.state(fighter).started(), Some(Tick::new(0)));
    assert_eq!(fight.sim.health(dummy), 100);
    fight.sim.run_until(3);
    assert_eq!(fight.sim.health(dummy), 70);
    assert_eq!(fight.state(fighter).ready_at(), Tick::new(5));
    assert_eq!(fight.state(fighter).started(), None);
    let attackers = |fight: &Fight| {
        let attackers = fight.sim.try_get::<RecentAttackers>(dummy).unwrap();
        attackers.iter().collect::<Vec<_>>()
    };
    let attack = |source, tick| RecentAttack {
        source,
        tick: Tick::new(tick),
    };
    assert_eq!(attackers(&fight), [attack(fighter, 2)]);
    fight.sim.run_until(8);
    assert_eq!(attackers(&fight), [attack(fighter, 7)]);
    fight.sim.run_until(17);
    assert_eq!(fight.sim.health(dummy), 10);
    fight.sim.run_until(18);
    assert!(fight.sim.try_get::<Pools>(dummy).is_none());
    assert_eq!(fight.state(fighter).target(), Some(dummy));
    // The next tick finds the target gone and drops it.
    fight.sim.run_until(19);
    assert_eq!(fight.state(fighter).target(), None);

    // A list keeps each attacker once, by stable id, and forgets one that despawned: the dummy.
    let index = fight.sim.world.resource::<EntityIndex>();
    let mut recent = RecentAttackers::default();
    recent.record(dummy, Tick::new(3), index);
    recent.record(fighter, Tick::new(4), index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 4)]);
    recent.record(fighter, Tick::new(6), index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 6)]);
    // One that despawned stays while known attackers strike again, and goes when a new one
    // comes: the list never outgrows the units of the match.
    let held = |attacks: &[RecentAttack]| {
        let bytes = postcard::to_allocvec(attacks).unwrap();
        postcard::from_bytes::<RecentAttackers>(&bytes).unwrap()
    };
    let mut recent = held(&[attack(dummy, 3), attack(fighter, 4)]);
    recent.record(fighter, Tick::new(7), index);
    let attacks = recent.iter().collect::<Vec<_>>();
    assert_eq!(attacks, [attack(dummy, 3), attack(fighter, 7)]);
    let mut recent = held(&[attack(dummy, 3)]);
    recent.record(fighter, Tick::new(8), index);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 8)]);
}

#[test]
fn a_range_counts_from_the_edge_of_each_body_in_the_maps_metric() {
    // A fighter of range 2 at x = 0, and a dummy 3 m off: out of range from center to center, in
    // range once each has a body of 0.5 m, as 3 ≤ 2 + 0.5 + 0.5; a bit farther, out again. Up at
    // y = 4, the dummy is still 3 m off on a planar map, and 5 m off on a spatial one.
    let half = Num::HALF;
    for (dummy_x, dummy_y, bodies, metric, starts) in [
        (Num::int(3), 0, false, Metric::Planar, false),
        (Num::int(3), 0, true, Metric::Planar, true),
        (Num::int(3) + Num::EPSILON, 0, true, Metric::Planar, false),
        (Num::int(3), 4, true, Metric::Planar, true),
        (Num::int(3), 0, true, Metric::Spatial, true),
        (Num::int(3), 4, true, Metric::Spatial, false),
    ] {
        let mut fight = Fight::new();
        fight.sim.world.insert_resource(metric);
        let fighter = fight.unit(Team::new(0), at(0, 0, 0), fighter());
        let dummy_at = Position::new(Vec3::new(dummy_x, Num::int(dummy_y), Num::ZERO)).unwrap();
        let dummy = fight.unit(Team::new(1), dummy_at, dummy());
        if bodies {
            for unit in [fighter, dummy] {
                fight.sim.insert(unit, Body::new(half).unwrap());
            }
        }
        fight.attack(fighter, dummy);
        fight.sim.run_until(1);
        // An attack in range starts in tick 0, its first.
        let started = fight.state(fighter).started();
        assert_eq!(
            started,
            starts.then_some(Tick::ZERO),
            "{dummy_x:?} {dummy_y} {bodies} {metric:?}"
        );
    }
}

#[test]
fn a_unit_attacks_with_its_first_weapon_whose_filter_selects_the_target() {
    // An RTS unit with a ground weapon of 1 m and no windup, dealing its stat at place 1, 10,
    // then an air weapon of 5 m and a windup of 1 tick, dealing its stat at place 2, 25; both at
    // its rate of 6 attacks a second, a period of 30 ÷ 6 = 5 ticks.
    let mut fight = Fight::new();
    let types = ["ground", "air", "structure"].map(|tag| {
        let data = UnitTypeData::tagged(&[tag]);
        Units::load_type(&mut fight.sim.world, TypeScope::Mode, tag, &data)
    });
    let hover = UnitTypeData::tagged(&["ground", "air"]);
    let hover = Units::load_type(&mut fight.sim.world, TypeScope::Mode, "hover", &hover);
    let weapon = |fight: &mut Fight, aim: &str, range, windup, damage| {
        let view = fight.sim.world.non_send::<View>().clone();
        let aim = view
            .resolve_filter(&FilterData::parse(aim).unwrap())
            .unwrap();
        let weapon = TestWeapon {
            damage,
            ..TestWeapon::new(aim, Range::Meters(Num::int(range)), Ticks::new(windup))
        };
        internals::weapon(&mut fight.sim.world, weapon)
    };
    let ground = weapon(&mut fight, "enemies:ground", 1, 0, StatId::new(1));
    let air = weapon(&mut fight, "enemies:air", 5, 1, StatId::new(2));
    let mut stats = UnitStats::default();
    stats
        .refill()
        .extend([Num::int(6), Num::int(10), Num::int(25)]);
    let slots = ActionSlots::new([ground, air].map(|weapon| (weapon, SlotKind::new(0), 1)));
    let unit = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    fight.sim.insert(unit, (slots, stats));
    let [ground_at, air_at, structure_at, hover_at] = [1, 4, 1, 1];
    let targets = [
        (types[0], ground_at),
        (types[1], air_at),
        (types[2], structure_at),
        (hover, hover_at),
    ]
    .map(|(unit_type, x)| {
        let target = fight.unit(Team::new(1), at(x, 0, 0), dummy());
        let entity = fight.sim.entity(target);
        fight.sim.insert(target, unit_type);
        UnitTags::give_type_tags(&mut fight.sim.world, entity);
        target
    });
    let [ground_unit, air_unit, structure, hovering] = targets;
    let healths = |fight: &Fight| targets.map(|target| fight.sim.health(target));
    let underway = |fight: &Fight| {
        let slots = fight.sim.try_get::<ActionSlots>(unit).unwrap();
        slots.in_progress().map(InProgress::slot)
    };

    // The air unit, 4 m off, only the air weapon selects: it starts in tick 0 and strikes in
    // tick 1, for 25, and is ready again in tick 0 + 5.
    fight.attack(unit, air_unit);
    fight.sim.run_until(1);
    assert_eq!(underway(&fight), Some(1));
    fight.sim.run_until(2);
    assert_eq!(healths(&fight), [100, 75, 100, 100]);
    let slots = fight.sim.try_get::<ActionSlots>(unit).unwrap();
    assert_eq!(slots.slot(1).unwrap().ready_at, Tick::new(5));

    // The ground unit, 1 m off: the ground weapon, ready, strikes in tick 2 itself, for 10, while
    // the air weapon cools down.
    fight.attack(unit, ground_unit);
    fight.sim.run_until(3);
    assert_eq!(healths(&fight), [90, 75, 100, 100]);
    let slots = fight.sim.try_get::<ActionSlots>(unit).unwrap();
    assert_eq!(slots.slot(0).unwrap().ready_at, Tick::new(7));

    // No weapon selects the structure: nothing starts while it is the target.
    fight.attack(unit, structure);
    for tick in 4..=8 {
        fight.sim.run_until(tick);
        assert_eq!(underway(&fight), None, "tick {tick}");
    }
    assert_eq!(healths(&fight), [90, 75, 100, 100]);

    // Both weapons select a hovering unit, ground and air, 1 m off: the first, the ground one,
    // ready since tick 7, strikes in tick 8 for 10.
    fight.attack(unit, hovering);
    fight.sim.run_until(9);
    assert_eq!(healths(&fight), [90, 75, 100, 90]);

    // A weapon not learned is none: a unit √2 m off the hovering one, its ground weapon at rank
    // 0, starts with the air one in tick 9 and strikes in tick 10 for 25, while the first unit's
    // ground weapon waits for tick 13.
    let second = fight.unit(Team::new(0), at(0, 0, 1), dummy());
    let mut stats = UnitStats::default();
    stats
        .refill()
        .extend([Num::int(6), Num::int(10), Num::int(25)]);
    let kind = SlotKind::new(0);
    let slots = ActionSlots::new([(ground, kind, 0), (air, kind, 1)]);
    fight.sim.insert(second, (slots, stats));
    fight.attack(second, hovering);
    fight.sim.run_until(11);
    assert_eq!(healths(&fight), [90, 75, 100, 65]);
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
    let stunned = &[Block::Move, Block::Attack, Block::Cast, Block::Use];
    fight.sim.block_at(late, 2, SimSet::Move, stunned);
    fight.sim.set_blocks(late, &[]);
    fight.sim.run_until(1);
    fight.sim.set_blocks(early, &[Block::Attack]);
    fight.sim.run_until(2);
    assert_eq!(fight.state(early).started(), None);
    assert_eq!(fight.state(late).started(), Some(Tick::new(0)));
    fight.sim.run_until(3);
    assert_eq!(fight.sim.health(dummy), 100);
    for unit in [early, late] {
        let state = fight.state(unit);
        assert_eq!(
            (state.target(), state.started(), state.ready_at()),
            (Some(dummy), None, Tick::new(0))
        );
    }
    // The states end before tick 3: both start again in tick 3, ready at once as no strike spent
    // the period, and strike in tick 5, 100 − 2 × 30 = 40, ready again in tick 3 + 5 = 8.
    fight.sim.set_blocks(early, &[]);
    fight.sim.set_blocks(late, &[]);
    fight.sim.run_until(4);
    assert_eq!(fight.state(early).started(), Some(Tick::new(3)));
    fight.sim.run_until(6);
    assert_eq!(fight.sim.health(dummy), 40);
    assert_eq!(fight.state(late).ready_at(), Tick::new(8));
}

#[test]
fn a_weapons_cost_is_checked_as_it_starts_and_strikes_and_paid_in_pools_and_resources() {
    // A weapon of 2 m, a windup of 2 ticks, 6 attacks a second, a period of 5 ticks, and 10
    // damage, that costs 4 mana and 2 gold; its unit has 100 mana, and its player 5 gold.
    let mut fight = Fight::new();
    let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
    let mut resources = PlayerResources::new(1, 1);
    resources.add(PlayerSlot::new(0), gold, 5).unwrap();
    fight.sim.world.insert_resource(resources);
    let mana = PoolId::new(1).unwrap();
    let weapon = TestWeapon {
        cost: PoolCost::new([(mana, Num::int(4))]),
        resource_cost: Some(ResourceAmount {
            resource: gold,
            amount: 2,
        }),
        ..TestWeapon::new(
            Filter::of_relation(Relation::Enemies),
            Range::Meters(Num::int(2)),
            Ticks::new(2),
        )
    };
    let weapon = internals::weapon(&mut fight.sim.world, weapon);
    let mut stats = UnitStats::default();
    stats.refill().extend([Num::int(6), Num::int(10)]);
    let unit = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    let dummy = fight.unit(Team::new(1), at(1, 0, 0), dummy());
    let pools = Pools::new([(PoolId::FIRST, Num::int(100)), (mana, Num::int(100))]).unwrap();
    let slots = ActionSlots::new([(weapon, SlotKind::new(0), 1)]);
    let owner = Owner::new(PlayerSlot::new(0));
    fight.sim.insert(unit, (slots, stats, pools, owner));
    fight.attack(unit, dummy);
    let paid = |fight: &Fight| {
        let pools = fight.sim.try_get::<Pools>(unit).copied().unwrap();
        let mana = pools
            .current(mana)
            .unwrap()
            .to_int()
            .expect("a whole amount");
        let gold = fight
            .sim
            .world
            .resource::<PlayerResources>()
            .amount(PlayerSlot::new(0), gold);
        (mana, gold, fight.sim.health(dummy))
    };
    let add_gold = |fight: &mut Fight, amount| {
        let mut resources = fight.sim.world.resource_mut::<PlayerResources>();
        resources.add(PlayerSlot::new(0), gold, amount).unwrap();
    };

    // Attacks start in ticks 0 and 5 and strike 2 ticks later, each paying 4 mana and 2 gold:
    // 100 to 96 and 92, 5 to 3 and 1, and the dummy 100 to 90 and 80.
    fight.sim.run_until(3);
    assert_eq!(paid(&fight), (96, 3, 90));
    fight.sim.run_until(8);
    assert_eq!(paid(&fight), (92, 1, 80));

    // Ready in tick 10, it cannot afford 2 gold of 1, so nothing starts.
    fight.sim.run_until(11);
    assert_eq!(fight.state(unit).started(), None);

    // With 2 gold an attack starts in tick 11, to strike in tick 13; the gold goes before it
    // strikes, so the checks fail again at the strike: it stops and spends nothing.
    add_gold(&mut fight, 1);
    fight.sim.run_until(12);
    assert_eq!(fight.state(unit).started(), Some(Tick::new(11)));
    add_gold(&mut fight, -2);
    fight.sim.run_until(14);
    assert_eq!(paid(&fight), (92, 0, 80));
    assert_eq!(fight.state(unit).started(), None);
    assert_eq!(fight.state(unit).ready_at(), Tick::new(10));
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
    fight.sim.run_until(2);
    assert!(fight.sim.try_get::<Pools>(prey).is_none());
    assert_eq!(fight.state(slow).started(), Some(Tick::new(0)));
    fight.sim.run_until(3);
    assert_eq!(fight.state(slow), Attack::default());
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
    fight.sim.run_until(2);
    for (unit, other) in [(first, second), (second, first)] {
        assert_eq!(fight.sim.health(unit), 0);
        assert!(fight.sim.try_get::<Dead>(unit).is_some());
        assert_eq!(fight.state(unit).target(), None);
        let attackers = fight.sim.try_get::<RecentAttackers>(unit).unwrap();
        let attack = RecentAttack {
            source: other,
            tick: Tick::new(1),
        };
        assert_eq!(attackers.iter().collect::<Vec<_>>(), [attack]);
    }

    // A dead unit is no target.
    let third = fight.unit(Team::new(1), at(0, 0, 1), fighter());
    fight.attack(third, first);
    fight.sim.run_until(3);
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
    fight.sim.insert(dead, Dead);
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
        fight
            .sim
            .set_blocks(unit, cases[usize::try_from(x).unwrap()].0);
        unit
    });

    let enemy_at = |fight: &mut Fight, team: Team, target: StableId| {
        fight
            .sim
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
    let mut relations = fight.sim.world.resource_mut::<Relations>();
    relations.set(Team::new(2), Team::new(0), Attitude::Friendly, true);
    relations.set(Team::new(3), Team::new(0), Attitude::Neutral, true);
    let related = [2, 3, 200].map(|team| enemy_at(&mut fight, Team::new(team), far));
    assert_eq!(related, [None, Some(at(6, 0, 0)), Some(at(6, 0, 0))]);
    let entity = fight.sim.entity(high);
    fight.sim.world.despawn(entity);
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
    fight.sim.run_until(3);
    assert!(fight.sim.try_get::<Dead>(doomed).is_some());

    // A restore loads the match's books first, as the packages give them: the same weapons, in
    // the same order.
    let mut restored = Fight::new();
    let _weapon = self::fighter().bundle(&mut restored.sim.world, Team::new(0));
    let doomed_kind = combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay);
    let _weapon = doomed_kind.bundle(&mut restored.sim.world, Team::new(1));
    fight.sim.restore_into(&mut restored.sim);
    assert_eq!(
        restored.sim.try_get::<ActionSlots>(fighter),
        fight.sim.try_get::<ActionSlots>(fighter)
    );

    // The fighter's attack winds up 2 ticks: under way, it resolves in tick 2 at the soonest,
    // where it started in tick 0, and at the limit at the latest, as its weapon is ready at the
    // latest.
    let entity = restored.sim.entity(fighter);
    let past = Tick::LIMIT.get() + 1;
    let slots = |resolves_at: u64, ready_at: u64| {
        let mut slots = restored.sim.get::<ActionSlots>(fighter).clone();
        slots.set_attack_target(Some(doomed));
        slots.start_attack(0, Tick::new(resolves_at));
        slots.cool_down(0, Tick::new(ready_at));
        slots.check(&restored.sim.world, entity)
    };
    assert!(slots(2, 0) && slots(Tick::LIMIT.get(), Tick::LIMIT.get()));
    assert!(!slots(1, 0));
    assert!(!slots(past, 0) && !slots(2, past));
    // An attack under way at a rank its weapon lacks is refused, not read past the weapon's
    // ranks: 0, before it is learned, and 30, past its one.
    let weapon = restored
        .sim
        .get::<ActionSlots>(fighter)
        .slot(0)
        .unwrap()
        .action;
    let ranked = |rank| {
        let mut slots = ActionSlots::new([(weapon, SlotKind::new(0), rank)]);
        slots.set_attack_target(Some(doomed));
        slots.start_attack(0, Tick::new(2));
        slots.check(&restored.sim.world, entity)
    };
    assert!(ranked(1) && !ranked(0) && !ranked(30));
}

#[test]
fn stats_out_of_their_limits_are_refused() {
    let life = |max| Pools::new([(PoolId::FIRST, max)]);
    assert_eq!(life(Num::ZERO), None);
    assert_eq!(
        life(Num::EPSILON).and_then(|pools| pools.current(PoolId::FIRST)),
        Some(Num::EPSILON)
    );

    // A snapshot's values pass the same limits.
    let health = |current: i64, max: i64| {
        let mut meters = [None; Pools::LIMIT];
        meters[0] = Some((Num::int(current), Num::int(max), 0_u32));
        let bytes = postcard::to_allocvec(&meters).unwrap();
        postcard::from_bytes::<Pools>(&bytes).ok()
    };
    assert!(health(0, 1).is_some() && health(1, 1).is_some());
    for (current, max) in [(-1, 1), (2, 1), (0, 0)] {
        assert_eq!(health(current, max), None, "{current} of {max}");
    }
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
            fight
                .sim
                .world
                .insert_resource(AssistWindow(Ticks::new(ticks)));
        }
        let team = Team::new(0);
        let [first, killer] = [4, 6].map(|x| fight.unit(team, at(x, 0, 0), fighter()));
        let slow_one = fight.unit(team, at(5, 0, 0), slow);
        let stays = combatant(130, 0, 0, 1, 0).on_death(OnDeath::Stay);
        let hero = fight.unit(Team::new(1), at(5, 0, 1), stays);
        let owner = Owner::new(PlayerSlot::new(3));
        fight.sim.insert(hero, owner);
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
        fight.sim.run_until(7);
        assert_eq!(fight.sim.health(hero), 40, "{window:?}");
        fight.sim.run_until(8);
        // Each death holds its tick, 7, and the unit as it was: its team, and its owner, a
        // record that outlives the creep.
        let deaths = fight.sim.world.resource::<Deaths>();
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
        assert!(fight.sim.try_get::<Dead>(hero).is_some());
        assert!(fight.sim.try_get::<Pools>(creep).is_none());
    }

    // The hero comes back at the start of its respawn tick, 10, at its spawn point with full
    // health and no attacker on record; the attackers dropped it when it died.
    let mut fight = Fight::new();
    let attacker = fight.unit(Team::new(0), at(4, 0, 0), fighter());
    let stays = combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay);
    let hero = fight.unit(Team::new(1), at(5, 0, 0), stays);
    fight.sim.insert(hero, SpawnPoint::new(at(-3, 0, 2)));
    fight.attack(attacker, hero);
    fight.sim.run_until(3);
    assert!(fight.sim.try_get::<Dead>(hero).is_some());
    let at_tick = Tick::new(10);
    fight.sim.insert(hero, Respawn { at: at_tick });
    fight.sim.run_until(10);
    assert!(fight.sim.try_get::<Dead>(hero).is_some());
    fight.sim.run_until(11);
    assert!(
        fight.sim.try_get::<Dead>(hero).is_none() && fight.sim.try_get::<Respawn>(hero).is_none()
    );
    assert_eq!(
        fight.sim.try_get::<Position>(hero).copied(),
        Some(at(-3, 0, 2))
    );
    assert_eq!(fight.sim.health(hero), 30);
    assert_eq!(
        fight
            .sim
            .try_get::<RecentAttackers>(hero)
            .map(|r| r.iter().count()),
        Some(0)
    );
    assert_eq!(fight.state(attacker).target(), None);
}

/// Loads a book of the stats `[combat]` binds, with no limits, and binds them: the heal scale
/// `healing_received_pct`, and leech `life_steal` from attacks and `spell_vamp` from the rest,
/// in that order among a unit's values; and `health`, the first pool, as the life pool.
fn load_damage_stats(world: &mut World) {
    let [heal, attack, other] =
        ["healing_received_pct", "life_steal", "spell_vamp"].map(|name| Stat::named(name).unwrap());
    let rules: BTreeMap<_, _> = [&heal, &attack, &other]
        .map(|stat| (stat.clone(), StatRule::default()))
        .into();
    let book = StatBook::new(&rules, [], Num::int(10));
    let health = DeclaredName::new("health").unwrap();
    let combat = CombatRules {
        life: Some(health.clone()),
        leech: Some(Leech {
            attack: Some(attack),
            other: Some(other),
        }),
        heal_scale: Some(heal),
        ..CombatRules::default()
    };
    let max = Stat::named("health").unwrap();
    let pools = BTreeMap::from([(health, PoolData { max, regen: None })]);
    world.insert_resource(LifePool(combat.life_pool(&pools).unwrap()));
    world.insert_resource(CombatBindings::new(&combat, &book));
    Stats::load_book(world, book);
}

/// A modifier of no stats, tags or script, as each shield the shield test gives is.
fn shield_data() -> ModifierData {
    ModifierData {
        ..ModifierData::default()
    }
}

impl Fight {
    /// Gives `id` the values of `load_damage_stats`' stats.
    fn stats(&mut self, id: StableId, values: [Num; 3]) {
        let mut stats = UnitStats::default();
        stats.refill().extend(values);
        self.sim.insert(id, stats);
    }

    /// Queues `amount` of damage from `source` to `target`, dealt by `cause`.
    fn damage(
        &mut self,
        source: Option<StableId>,
        target: StableId,
        amount: i64,
        cause: DamageCause,
    ) {
        self.sim
            .world
            .resource_mut::<PassQueue>()
            .push_damage(Damage {
                source,
                target,
                amount: Num::int(amount),
                kind: DamageKind::new(0),
                cause,
                ability: None,
                depth: 0,
                hit: None,
            });
    }
}

const ATTACK: DamageCause = DamageCause::Attack {
    roll: Num::ZERO,
    rank: 1,
};

#[test]
fn the_pass_deals_damage_in_its_order_and_credits_the_kill() {
    let mut fight = Fight::new();
    fight
        .sim
        .world
        .insert_resource(AssistWindow(Ticks::new(10)));
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
    fight.sim.run_until(1);
    assert_eq!(fight.sim.health(target), 0);
    let deaths = fight.sim.world.resource::<Deaths>();
    let died: Vec<_> = deaths
        .iter()
        .map(|death| (death.fallen.unit, death.killer, death.assisters.to_vec()))
        .collect();
    assert_eq!(died, [(target, Some(b), vec![a])]);

    // At zero health it takes nothing more, records no attacker, and heals nothing.
    let c = fight.unit(Team::new(0), at(3, 0, 0), dummy());
    fight.damage(Some(c), target, 10, ATTACK);
    fight.sim.run_until(2);
    DamagePass::heal(&mut fight.sim.world, target, Num::int(50));
    assert_eq!(fight.sim.health(target), 0);
    let attackers = fight.sim.try_get::<RecentAttackers>(target).unwrap();
    assert!(attackers.iter().all(|attack| attack.source != c));
    assert_eq!(fight.sim.world.resource::<Deaths>().iter().count(), 0);

    // An invulnerable unit takes nothing and records no attacker; untargetable, it takes all.
    let guarded = fight.unit(Team::new(1), at(4, 0, 0), dummy());
    let hidden = fight.unit(Team::new(1), at(5, 0, 0), dummy());
    fight
        .sim
        .set_blocks(guarded, &[Block::Target, Block::Damage]);
    fight.sim.set_blocks(hidden, &[Block::Target]);
    fight.damage(Some(c), guarded, 10, DamageCause::Effect);
    fight.damage(Some(c), hidden, 10, DamageCause::Effect);
    fight.sim.run_until(3);
    assert_eq!(
        (fight.sim.health(guarded), fight.sim.health(hidden)),
        (100, 90)
    );
    let attackers = fight.sim.try_get::<RecentAttackers>(guarded).unwrap();
    assert_eq!(attackers.iter().count(), 0);
}

#[test]
fn shields_absorb_soonest_end_first_and_vamps_heal_from_health_taken() {
    let mut fight = Fight::new();
    load_damage_stats(&mut fight.sim.world);
    let half = Num::HALF;
    let source = fight.unit(Team::new(0), at(0, 0, 0), dummy());
    let target = fight.unit(Team::new(1), at(1, 0, 0), dummy());
    // Heals halved, life steal 0.5 and spell vamp 0.25; at 40 of 100.
    fight.stats(source, [-half, half, Num::ONE / 4]);
    let entity = fight.sim.entity(source);
    let mut pools = fight.sim.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, Num::int(60));
    let shield = |id: u16, until: Option<u64>, amount: i64| NewInstance {
        lifetime: Lifetime::new(
            None,
            until.map_or(Ends::Never, |until| Ends::At(Tick::new(until))),
        ),
        shield: Some(Num::int(amount)),
        ..NewInstance::bare(ModifierId::new(id), None)
    };
    for name in ["first", "second", "third"] {
        Stats::load_modifier(&mut fight.sim.world, 0, name, &shield_data(), None);
    }
    let shields = [
        shield(0, None, 100),
        shield(1, Some(50), 20),
        shield(2, Some(30), 10),
    ]
    .map(Application::refresh);
    fight.sim.insert(target, Modifiers::bundle(shields));
    let shields = |fight: &Fight| {
        let clocks = fight.sim.try_get::<ModifierClocks>(target).unwrap();
        let held = fight.sim.try_get::<Modifiers>(target).unwrap().len();
        (0..held)
            .map(|at| clocks.shield(at).unwrap())
            .collect::<Vec<_>>()
    };

    // 35: the shield that ends in tick 30 spends its 10, the one of tick 50 its 20, and the one
    // with no end 5 of its 100; health takes nothing, and the source heals nothing.
    fight.damage(Some(source), target, 35, ATTACK);
    fight.sim.run_until(1);
    assert_eq!(shields(&fight), [Num::int(95)]);
    assert_eq!(fight.sim.life(target), Num::int(100));
    assert_eq!(fight.sim.life(source), Num::int(40));
    // An attack of 100: the last shield's 95, then 5 off health, which life steals 5 × 0.5,
    // halved: 1.25. Then a spell of 40, all off health: 40 × 0.25, halved, 5. 40 + 1.25 + 5.
    fight.damage(Some(source), target, 100, ATTACK);
    fight.damage(Some(source), target, 40, DamageCause::Effect);
    fight.sim.run_until(2);
    assert_eq!(shields(&fight), []);
    assert_eq!(fight.sim.life(target), Num::int(55));
    assert_eq!(fight.sim.life(source), Num::int(185) / 4);
    // A heal of 10 is halved too; one past the maximum stops at it.
    DamagePass::heal(&mut fight.sim.world, source, Num::int(10));
    assert_eq!(fight.sim.life(source), Num::int(205) / 4);
    DamagePass::heal(&mut fight.sim.world, source, Num::int(1000));
    assert_eq!(fight.sim.life(source), Num::int(100));
    // A heal scale at the largest number, one past which no number holds, scales as the largest:
    // a heal of 1 from 40 fills the pool.
    fight.stats(source, [Num::MAX, half, Num::ONE / 4]);
    fight
        .sim
        .get_mut::<Pools>(source)
        .take(PoolId::FIRST, Num::int(60));
    DamagePass::heal(&mut fight.sim.world, source, Num::ONE);
    assert_eq!(fight.sim.life(source), Num::int(100));
    fight.stats(source, [-half, half, Num::ONE / 4]);
    // Without the bindings the same stats do nothing: at 50, an attack of 10 heals the source
    // nothing, and a heal of 10 is whole.
    super::internals::bind_life(&mut fight.sim.world, PoolId::FIRST);
    let entity = fight.sim.entity(source);
    let mut pools = fight.sim.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, Num::int(50));
    fight.damage(Some(source), target, 10, ATTACK);
    fight.sim.run_until(3);
    assert_eq!(fight.sim.life(target), Num::int(45));
    assert_eq!(fight.sim.life(source), Num::int(50));
    DamagePass::heal(&mut fight.sim.world, source, Num::int(10));
    assert_eq!(fight.sim.life(source), Num::int(60));
    // A restore reaches the pool it names, unscaled: a second pool at 50 of 100 takes 20 more,
    // and the life pool keeps its 60.
    let mana = PoolId::new(1).unwrap();
    let mut pools = fight.sim.get_mut::<Pools>(source);
    *pools = Pools::new([(PoolId::FIRST, Num::int(100)), (mana, Num::int(100))]).unwrap();
    pools.take(PoolId::FIRST, Num::int(40));
    pools.take(mana, Num::int(50));
    DamagePass::restore(&mut fight.sim.world, source, mana, Num::int(20));
    let pools = fight.sim.try_get::<Pools>(source).copied().unwrap();
    assert_eq!(
        (pools.current(PoolId::FIRST), pools.current(mana)),
        (Some(Num::int(60)), Some(Num::int(70)))
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
            let mut slots = fight.sim.get_mut::<ActionSlots>(attacker);
            slots.set_attack_target(Some(dummy));
            slots.start_attack(0, Tick::new(0));
        }
        fight.sim.world.resource_mut::<PassQueue>().clear();
        fight.sim.world.run_system_once(strike).unwrap();
        let queue = fight.sim.world.resource::<PassQueue>();
        let mut rolls: Vec<_> = (0..attackers.len())
            .map(|at| {
                let Some(PassEntry::Damage(damage)) = queue.get(at) else {
                    panic!("an attack queues damage");
                };
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
    let half = Num::HALF;
    let crits: Vec<_> = first.iter().map(|&roll| roll < half).collect();
    let old = source(0);
    let rolled = |&id: &StableId| old.open(ROLL_STREAM, id.get()).chance(half);
    assert_eq!(crits, attackers.iter().map(rolled).collect::<Vec<_>>());
    assert!(crits.contains(&true) && crits.contains(&false));
    fight
        .sim
        .world
        .insert_resource(SimRng::new(SegmentSeed::new([1; 32])));
    let second = rolls(&mut fight);
    assert_eq!(second, expected(1));
    assert_ne!(first, second);
}
