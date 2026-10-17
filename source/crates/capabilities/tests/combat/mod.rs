use std::collections::BTreeMap;
use std::num::NonZeroU16;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::system::RunSystemOnce;
use bevy_ecs::world::World;
use campfire_actions::internals::TestWeapon;
use campfire_actions::{
    ActionProgress, ActionRange, ActionSlot, ActionSlots, Actions, ClipRule, InProgress, SlotKind,
    SlotTimes, Source, Targets, TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_codec::Binary;
use campfire_combat::internals::{Armed, Attack};
use campfire_combat::*;
use campfire_common::{PlayerSlot, SegmentSeed, Tick, Ticks};
use campfire_geometry::Metric;
use campfire_math::{Num, Rng, Vec3};
use campfire_mode::Mode;
use campfire_players::{Players, PlayersWorld, ResourceAmount};
use campfire_script::{Filter, ScriptBudgets, ScriptLimits, View};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{
    Capability, EntityIndex, Position, SimComponent, SimEdge, SimRng, SimSet, SimTick, SimUpdate,
    SimWorld, StableId, Unpredicted,
};
use campfire_stats::{
    Application, Ends, Life, Lifetime, ModifierClocks, ModifierData, Modifiers, NewInstance,
    PoolBook, PoolCost, PoolId, Pools, StatBook, StatId, StatRule, Stats, StatsSet,
};
use campfire_units::{BodyForm, Dead, MapBook, Owner, SpawnPoint, UnitTypeCore, Units, UnitsWorld};
use campfire_values::{
    ActionId, Block, DamageKind, DeclaredName, FilterData, ModifierId, NameId, Names, PackageIndex,
    Rank, Relation, RelationSet, ResourceId, Stat, Team, TypeScope, UnitTags, UnitType,
    VisionShare,
};

/// `health`, and `damage` within `range`, a windup and a period in ticks.
const fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Armed {
    Armed::melee(
        Num::int(health),
        Num::int(range),
        windup,
        period,
        Num::int(damage),
    )
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
const fn fighter() -> Armed {
    combatant(100, 2, 2, 5, 30)
}

/// 100 health; never attacks.
const fn dummy() -> Armed {
    combatant(100, 0, 0, 1, 0)
}

/// A match of stats and combat, with the stat book a weapon reads.
fn fight_match() -> Match {
    let budgets = ScriptBudgets::new(ScriptLimits::ROOMY, 1);
    let mut game = Match::server(&[Capability::Stats, Capability::Combat], budgets);
    Combat::load_weapon_stats(&mut game);
    game
}

/// The next type of the mode, of 100 health that despawns when it dies, with `slots` and the
/// stats `values` by their places.
fn wielder(game: &mut Match, slots: ActionSlots, values: &[Num]) -> UnitType {
    let unit_type = Units::load_next_type(game, &UnitTypeCore::default());
    Units::give_type(game, unit_type, Life::full(Num::int(100)));
    Units::give_type(game, unit_type, OnDeath::Despawn);
    Units::give_type(game, unit_type, slots);
    Stats::give_type_stats(game, unit_type, values);
    unit_type
}

/// Loads each of `armed` as the next type of the mode.
fn arm<const N: usize>(game: &mut Match, armed: [Armed; N]) -> [UnitType; N] {
    armed.map(|armed| armed.load(game))
}

#[test]
fn an_attack_winds_up_and_strikes_each_period() {
    let mut game = fight_match();
    let [fighter, dummy] = arm(&mut game, [fighter(), dummy()]);
    let fighter = Mode::spawn(&mut game, fighter, (4, 0, 0), Team::new(0), ());
    let dummy = Mode::spawn(&mut game, dummy, (6, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, fighter, dummy);

    // In range from tick 0: attacks start in ticks 0, 5, 10 and 15, and strike 2 ticks later,
    // taking 100 to 70, 40, 10 and 0: the dummy despawns in tick 17.
    game.run_until(2);
    assert_eq!(Attack::of(&game, fighter).started(), Some(Tick::new(0)));
    assert_eq!(Stats::health(&game, dummy), 100);
    game.run_until(3);
    assert_eq!(Stats::health(&game, dummy), 70);
    assert_eq!(Attack::of(&game, fighter).ready_at(), Tick::new(5));
    assert_eq!(Attack::of(&game, fighter).started(), None);
    let attackers = |game: &Match| {
        let attackers = game.try_part::<RecentAttackers>(dummy).unwrap();
        attackers.iter().collect::<Vec<_>>()
    };
    let attack = |source, tick| RecentAttack {
        source,
        tick: Tick::new(tick),
    };
    assert_eq!(attackers(&game), [attack(fighter, 2)]);
    // The attack that starts in tick 5 changes what is under way, not the slots' table, which
    // what keys on the slots does not visit for it.
    let written = |game: &Match| {
        let unit = game.entity(game.unit_entity(fighter));
        (
            unit.get_change_ticks::<ActionSlots>().unwrap().changed,
            unit.get_change_ticks::<ActionProgress>().unwrap().changed,
        )
    };
    game.run_until(5);
    let before = written(&game);
    game.run_until(6);
    let after = written(&game);
    assert_eq!((after.0 == before.0, after.1 == before.1), (true, false));
    game.run_until(8);
    assert_eq!(attackers(&game), [attack(fighter, 7)]);
    game.run_until(17);
    assert_eq!(Stats::health(&game, dummy), 10);
    game.run_until(18);
    assert!(game.try_part::<Life>(dummy).is_none());
    assert_eq!(Attack::of(&game, fighter).target(), Some(dummy));
    // The next tick finds the target gone and drops it.
    game.run_until(19);
    assert_eq!(Attack::of(&game, fighter).target(), None);

    // A list keeps each attacker once, by stable id.
    let index = game.resource::<EntityIndex>();
    let record = |recent: &mut RecentAttackers, source, tick| {
        recent.record(source, Tick::new(tick), index);
    };
    let mut recent = RecentAttackers::default();
    record(&mut recent, fighter, 4);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 4)]);
    record(&mut recent, fighter, 6);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 6)]);
    // One that despawned stays while known attackers strike again, and goes when a new one
    // comes: the list never outgrows the units of the match.
    let held = |attacks: &[RecentAttack]| {
        let bytes = Binary::encode(attacks);
        Binary::decode::<RecentAttackers>(&bytes).unwrap()
    };
    let mut recent = held(&[attack(fighter, 4), attack(dummy, 3)]);
    record(&mut recent, fighter, 7);
    let attacks = recent.iter().collect::<Vec<_>>();
    assert_eq!(attacks, [attack(fighter, 7), attack(dummy, 3)]);
    let mut recent = held(&[attack(dummy, 3)]);
    record(&mut recent, fighter, 8);
    assert_eq!(recent.iter().collect::<Vec<_>>(), [attack(fighter, 8)]);
}

/// Spawns a unit of a rifle of 5 damage every 3 ticks, 100 ms at 30 ticks a second, in a clip of
/// 3 shots that reloads in 21 ticks, 700 ms, at (4, 0), and a dummy of team 1 with `held` at
/// (6, 0), which the rifle attacks; the two.
fn rifle_fight(game: &mut Match, held: impl Bundle) -> [StableId; 2] {
    let [dummy] = arm(game, [dummy()]);
    let weapon = TestWeapon {
        rate: Source::Own,
        damage: Source::Own,
        period: Some(Ticks::new(3)),
        amount: Some(Num::int(5)),
        clip: Some(ClipRule {
            shots: NonZeroU16::new(3).unwrap(),
            reload: Ticks::new(21),
        }),
        ..TestWeapon::new(
            Filter::of_match_relations(game, RelationSet::Enemies),
            ActionRange::Meters(Num::int(2)),
            Ticks::ZERO,
        )
    };
    let rifle = Actions::add_weapon(game, weapon);
    let slots = ActionSlots::new([(rifle, SlotKind::new(0), Rank::new(1))]);
    let shooter = wielder(game, slots, &[]);
    let shooter = Mode::spawn(game, shooter, (4, 0, 0), Team::new(0), ());
    let dummy = Mode::spawn(game, dummy, (6, 0, 0), Team::new(1), held);
    Combat::attack(game, shooter, dummy);
    [shooter, dummy]
}

/// The ticks the attack of `shooter` is ready again from, each as it changes, over ticks 0 to 55;
/// and, of a world that deals damage, the ticks `dummy` lost life in.
fn rifle_ticks(game: &mut Match, [shooter, dummy]: [StableId; 2]) -> [Vec<u64>; 2] {
    let (mut readies, mut hits) = (Vec::new(), Vec::new());
    let mut health = Stats::health(game, dummy);
    for tick in 0..56 {
        game.run_until(tick + 1);
        let ready = Attack::of(game, shooter).ready_at().get();
        if readies.last() != Some(&ready) {
            readies.push(ready);
        }
        let now = Stats::health(game, dummy);
        if now != health {
            hits.push(tick);
            health = now;
        }
    }
    [readies, hits]
}

#[test]
fn a_weapon_of_its_own_amount_and_period_fires_its_clip_then_reloads() {
    // With no windup, the rifle strikes in ticks 0, 3 and 6, each ready 3 ticks after; the third
    // empties its clip, so it is ready again 21 ticks after it, in 27; then 30, 33, and 54 after
    // the second clip. Each strike takes 5 of the dummy's 100, its own amount, as the shooter
    // holds no stat: 7 strikes leave 65.
    let readies = [3, 6, 27, 30, 33, 54, 57];
    let mut game = fight_match();
    let fight = rifle_fight(&mut game, ());
    let [ready, hits] = rifle_ticks(&mut game, fight);
    assert_eq!(ready, readies);
    assert_eq!(hits, [0, 3, 6, 27, 30, 33, 54]);
    assert_eq!(Stats::health(&game, fight[1]), 65);
    // A predicting client, which holds the dummy as the server sent it and deals no damage,
    // counts the clip as the server does, with the count the server sends of its own shooter:
    // its rifle is ready again in the same ticks.
    let mut client = Match::predicting(&[Capability::Stats, Capability::Combat]);
    Combat::load_weapon_stats(&mut client);
    let fight = rifle_fight(&mut client, Unpredicted);
    client.insert_parts(fight[0], ClipShots::default());
    let [ready, hits] = rifle_ticks(&mut client, fight);
    assert_eq!(ready, readies);
    assert_eq!(hits, Vec::<u64>::new());
}

#[test]
fn every_unit_with_slots_counts_its_clips_from_its_spawn_or_a_grant() {
    // On a server, the shooter and the dummy, of a weapon of no clip, spawn with their counts,
    // empty. A unit of no slots holds none, and one once a grant gives it the rifle's.
    let mut game = fight_match();
    let bare = Units::load_next_type(&mut game, &UnitTypeCore::default());
    let [shooter, dummy] = rifle_fight(&mut game, ());
    let counts = |game: &Match, id| game.try_part::<ClipShots>(id).cloned();
    assert_eq!(counts(&game, shooter), Some(ClipShots::default()));
    assert_eq!(counts(&game, dummy), Some(ClipShots::default()));
    let rifle = game
        .part::<ActionSlots>(shooter)
        .slot(0)
        .unwrap()
        .action
        .unwrap();
    let bare = Mode::spawn(&mut game, bare, (9, 0, 0), Team::new(0), ());
    assert_eq!(counts(&game, bare), None);
    let granted = ActionSlots::new([(rifle, SlotKind::new(0), Rank::new(1))]);
    game.insert_parts(bare, TimedSlots::new(granted));
    assert_eq!(counts(&game, bare), Some(ClipShots::default()));
    // The shooter's rifle fires, and its count holds the shots.
    game.run_until(4);
    assert_ne!(counts(&game, shooter), Some(ClipShots::default()));
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
        let mut game = fight_match();
        game.resource_mut::<MapBook>().metric = metric;
        let types = [fighter(), dummy()].map(|armed| armed.load(&mut game));
        if bodies {
            for unit_type in types {
                Units::give_type(&mut game, unit_type, BodyForm::circle(half).unwrap());
            }
        }
        let fighter = Mode::spawn(&mut game, types[0], (0, 0, 0), Team::new(0), ());
        let dummy_at = Vec3::new(dummy_x, Num::int(dummy_y), Num::ZERO);
        let dummy = Mode::spawn(&mut game, types[1], dummy_at, Team::new(1), ());
        Combat::attack(&mut game, fighter, dummy);
        game.run_until(1);
        // An attack in range starts in tick 0, its first.
        let started = Attack::of(&game, fighter).started();
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
    let mut game = fight_match();
    let rules = ["attack_rate", "damage", "damage_air"]
        .map(|name| (Stat::named(name).unwrap(), StatRule::default()));
    Stats::load_rules(&mut game, &BTreeMap::from(rules));
    let types = ["ground", "air", "structure"].map(|tag| {
        let data = UnitTypeCore::tagged(&[tag]);
        Units::load_type(&mut game, TypeScope::Mode, tag, &data)
    });
    let hover = UnitTypeCore::tagged(&["ground", "air"]);
    let hover = Units::load_type(&mut game, TypeScope::Mode, "hover", &hover);
    let weapon = |game: &mut Match, aim: &str, range, windup, damage| {
        let view = game.non_send::<View>().clone();
        let aim = view
            .resolve_filter(&FilterData::parse(aim).unwrap())
            .unwrap();
        let weapon = TestWeapon {
            damage,
            ..TestWeapon::new(
                aim,
                ActionRange::Meters(Num::int(range)),
                Ticks::new(windup),
            )
        };
        Actions::add_weapon(game, weapon)
    };
    let ground = weapon(
        &mut game,
        "enemies:ground",
        1,
        0,
        Source::Stat(StatId::new(1)),
    );
    let air = weapon(&mut game, "enemies:air", 5, 1, Source::Stat(StatId::new(2)));
    // Two units of the RTS weapons: the first with both learned, the second with its ground
    // weapon at rank 0.
    let kind = SlotKind::new(0);
    let [rts, second] = [Rank::new(1), None].map(|ground_rank| {
        let slots = ActionSlots::new([(ground, kind, ground_rank), (air, kind, Rank::new(1))]);
        wielder(&mut game, slots, &[Num::int(6), Num::int(10), Num::int(25)])
    });
    for unit_type in types.into_iter().chain([hover]) {
        dummy().give(&mut game, unit_type);
    }
    let id = Mode::spawn(&mut game, rts, (0, 0, 0), Team::new(0), ());
    let [ground_at, air_at, structure_at, hover_at] = [1, 4, 1, 1];
    let targets = [
        (types[0], ground_at),
        (types[1], air_at),
        (types[2], structure_at),
        (hover, hover_at),
    ]
    .map(|(unit_type, x)| Mode::spawn(&mut game, unit_type, (x, 0, 0), Team::new(1), ()));
    let [ground_unit, air_unit, structure, hovering] = targets;
    let healths = |game: &Match| targets.map(|target| Stats::health(game, target));
    let underway = |game: &Match| {
        let progress = game.try_part::<ActionProgress>(id).unwrap();
        progress.in_progress().map(InProgress::slot)
    };

    // The air unit, 4 m off, only the air weapon selects: it starts in tick 0 and strikes in
    // tick 1, for 25, and is ready again in tick 0 + 5.
    Combat::attack(&mut game, id, air_unit);
    game.run_until(1);
    assert_eq!(underway(&game), Some(1));
    game.run_until(2);
    assert_eq!(healths(&game), [100, 75, 100, 100]);
    let times = game.try_part::<SlotTimes>(id).unwrap();
    assert_eq!(times.slot(1).unwrap().ready_at, Tick::new(5));

    // The ground unit, 1 m off: the ground weapon, ready, strikes in tick 2 itself, for 10, while
    // the air weapon cools down.
    Combat::attack(&mut game, id, ground_unit);
    game.run_until(3);
    assert_eq!(healths(&game), [90, 75, 100, 100]);
    let times = game.try_part::<SlotTimes>(id).unwrap();
    assert_eq!(times.slot(0).unwrap().ready_at, Tick::new(7));

    // No weapon selects the structure: nothing starts while it is the target.
    Combat::attack(&mut game, id, structure);
    for tick in 4..=8 {
        game.run_until(tick);
        assert_eq!(underway(&game), None, "tick {tick}");
    }
    assert_eq!(healths(&game), [90, 75, 100, 100]);

    // Both weapons select a hovering unit, ground and air, 1 m off: the first, the ground one,
    // ready since tick 7, strikes in tick 8 for 10.
    Combat::attack(&mut game, id, hovering);
    game.run_until(9);
    assert_eq!(healths(&game), [90, 75, 100, 90]);

    // A weapon not learned is none: a unit √2 m off the hovering one, its ground weapon at rank
    // 0, starts with the air one in tick 9 and strikes in tick 10 for 25, while the first unit's
    // ground weapon waits for tick 13.
    let second = Mode::spawn(&mut game, second, (0, 0, 1), Team::new(0), ());
    Combat::attack(&mut game, second, hovering);
    game.run_until(11);
    assert_eq!(healths(&game), [90, 75, 100, 65]);
}

#[test]
fn a_windup_its_attackers_states_stop_starts_again_and_spends_nothing() {
    let mut game = fight_match();
    let [fighter, dummy] = arm(&mut game, [fighter(), dummy()]);
    let early = Mode::spawn(&mut game, fighter, (0, 0, 0), Team::new(0), ());
    let late = Mode::spawn(&mut game, fighter, (0, 0, 1), Team::new(0), ());
    let dummy = Mode::spawn(&mut game, dummy, (1, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, early, dummy);
    Combat::attack(&mut game, late, dummy);
    // Both start in tick 0, to strike in tick 2. The early one is disarmed before tick 1: Act
    // interrupts its windup and keeps its target. The late one is stunned in tick 2's Move
    // stage, after Act, so its strike in Hit is interrupted instead.
    let stunned = &[Block::Move, Block::Attack, Block::Cast, Block::Use];
    Units::block_at(&mut game, late, 2, SimSet::Move, stunned);
    Units::set_blocks(&mut game, late, &[]);
    game.run_until(1);
    Units::set_blocks(&mut game, early, &[Block::Attack]);
    game.run_until(2);
    assert_eq!(Attack::of(&game, early).started(), None);
    assert_eq!(Attack::of(&game, late).started(), Some(Tick::new(0)));
    game.run_until(3);
    assert_eq!(Stats::health(&game, dummy), 100);
    for unit in [early, late] {
        let state = Attack::of(&game, unit);
        assert_eq!(
            (state.target(), state.started(), state.ready_at()),
            (Some(dummy), None, Tick::new(0))
        );
    }
    // The states end before tick 3: both start again in tick 3, ready at once as no strike spent
    // the period, and strike in tick 5, 100 − 2 × 30 = 40, ready again in tick 3 + 5 = 8.
    Units::set_blocks(&mut game, early, &[]);
    Units::set_blocks(&mut game, late, &[]);
    game.run_until(4);
    assert_eq!(Attack::of(&game, early).started(), Some(Tick::new(3)));
    game.run_until(6);
    assert_eq!(Stats::health(&game, dummy), 40);
    assert_eq!(Attack::of(&game, late).ready_at(), Tick::new(8));
}

#[test]
fn a_weapons_cost_is_checked_as_it_starts_and_strikes_and_paid_in_pools_and_resources() {
    // A weapon of 2 m, a windup of 2 ticks, 6 attacks a second, a period of 5 ticks, and 10
    // damage, that costs 4 mana and 2 gold; its unit has 100 mana, and its player 5 gold.
    let mut game = fight_match();
    let gold = ResourceId::new(0);
    Players::spawn(&mut game, 1, 1);
    game.player_resources_mut(PlayerSlot::new(0))
        .unwrap()
        .add(gold, 5)
        .unwrap();
    let mana = PoolId::new(1).unwrap();
    let weapon = TestWeapon {
        cost: PoolCost::new([(mana, Num::int(4))]),
        resource_cost: Some(ResourceAmount {
            resource: gold,
            amount: 2,
        }),
        ..TestWeapon::new(
            Filter::of_release_relations(RelationSet::Enemies),
            ActionRange::Meters(Num::int(2)),
            Ticks::new(2),
        )
    };
    let weapon = Actions::add_weapon(&mut game, weapon);
    let slots = ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]);
    let paying = wielder(&mut game, slots, &[Num::int(6), Num::int(10)]);
    let dummy = dummy().load(&mut game);
    let pools = Pools::new([(mana, Num::int(100))]).unwrap();
    Units::give_type(&mut game, paying, pools);
    let owner = Owner::new(PlayerSlot::new(0));
    let id = Mode::spawn(&mut game, paying, (0, 0, 0), Team::new(0), owner);
    let dummy = Mode::spawn(&mut game, dummy, (1, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, id, dummy);
    let paid = |game: &Match| {
        let pools = game.try_part::<Pools>(id).copied().unwrap();
        let mana = pools
            .current(mana)
            .unwrap()
            .to_int()
            .expect("a whole amount");
        let gold = game
            .player_resources(PlayerSlot::new(0))
            .unwrap()
            .amount(gold);
        (mana, gold, Stats::health(game, dummy))
    };
    let add_gold = |game: &mut Match, amount| {
        let mut resources = game.player_resources_mut(PlayerSlot::new(0)).unwrap();
        resources.add(gold, amount).unwrap();
    };

    // Attacks start in ticks 0 and 5 and strike 2 ticks later, each paying 4 mana and 2 gold:
    // 100 to 96 and 92, 5 to 3 and 1, and the dummy 100 to 90 and 80.
    game.run_until(3);
    assert_eq!(paid(&game), (96, 3, 90));
    game.run_until(8);
    assert_eq!(paid(&game), (92, 1, 80));

    // Ready in tick 10, it cannot afford 2 gold of 1, so nothing starts.
    game.run_until(11);
    assert_eq!(Attack::of(&game, id).started(), None);

    // With 2 gold an attack starts in tick 11, to strike in tick 13; the gold goes before it
    // strikes, so the checks fail again at the strike: it stops and spends nothing.
    add_gold(&mut game, 1);
    game.run_until(12);
    assert_eq!(Attack::of(&game, id).started(), Some(Tick::new(11)));
    add_gold(&mut game, -2);
    game.run_until(14);
    assert_eq!(paid(&game), (92, 0, 80));
    assert_eq!(Attack::of(&game, id).started(), None);
    assert_eq!(Attack::of(&game, id).ready_at(), Tick::new(10));
}

#[test]
fn a_toggle_an_attack_cannot_pay_with_its_own_cost_turns_off_and_the_attack_strikes() {
    // A weapon of 2 m, a windup of 2 ticks, a period of 5 ticks and 10 damage, that costs 4
    // mana, and a toggle, on, that costs 3 mana as each attack goes off; the unit has 12 mana.
    // The attack going off in tick 2 pays both, 12 − 3 − 4 = 5, and strikes. The one going off
    // in tick 7 cannot pay 3 + 4 of 5: the toggle turns off, and the attack pays its own 4 of
    // the 5 and strikes, so the dummy's 100 goes to 90, then 80.
    let mut game = fight_match();
    let mana = PoolId::new(1).unwrap();
    let weapon = TestWeapon {
        cost: PoolCost::new([(mana, Num::int(4))]),
        ..TestWeapon::new(
            Filter::of_release_relations(RelationSet::Enemies),
            ActionRange::Meters(Num::int(2)),
            Ticks::new(2),
        )
    };
    let weapon = Actions::add_weapon(&mut game, weapon);
    let toggle = Actions::add_attack_toggle(&mut game, PoolCost::new([(mana, Num::int(3))]));
    let slots = ActionSlots::new([
        (weapon, SlotKind::new(0), Rank::new(1)),
        (toggle, SlotKind::new(0), Rank::new(1)),
    ]);
    let paying = wielder(&mut game, slots, &[Num::int(6), Num::int(10)]);
    let dummy = dummy().load(&mut game);
    Units::give_type(
        &mut game,
        paying,
        Pools::new([(mana, Num::int(12))]).unwrap(),
    );
    let id = Mode::spawn(&mut game, paying, (0, 0, 0), Team::new(0), ());
    let dummy = Mode::spawn(&mut game, dummy, (1, 0, 0), Team::new(1), ());
    game.part_mut::<SlotTimes>(id).toggle_on(1, Tick::ZERO);
    Combat::attack(&mut game, id, dummy);
    let state = |game: &Match| {
        let pools = game.try_part::<Pools>(id).copied().unwrap();
        let mana = pools.current(mana).unwrap().to_int().unwrap();
        let on = game.part::<SlotTimes>(id).slot(1).unwrap().toggle.is_some();
        (mana, on, Stats::health(game, dummy))
    };
    game.run_until(3);
    assert_eq!(state(&game), (5, true, 90));
    game.run_until(8);
    assert_eq!(state(&game), (1, false, 80));
}

#[test]
fn a_windup_on_a_target_that_dies_spends_nothing() {
    let mut game = fight_match();
    let armed = [
        fighter(),
        combatant(100, 2, 1, 5, 30),
        combatant(30, 0, 0, 1, 0),
    ];
    let [slow, quick, prey] = arm(&mut game, armed);
    let slow = Mode::spawn(&mut game, slow, (0, 0, 0), Team::new(0), ());
    let quick = Mode::spawn(&mut game, quick, (0, 0, 1), Team::new(0), ());
    let prey = Mode::spawn(&mut game, prey, (1, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, slow, prey);
    Combat::attack(&mut game, quick, prey);

    // Both start in tick 0; the quick one strikes in tick 1 and kills the prey, before the slow
    // one's strike in tick 2. The slow one drops its target in tick 2 and is ready at once.
    game.run_until(2);
    assert!(game.try_part::<Life>(prey).is_none());
    assert_eq!(Attack::of(&game, slow).started(), Some(Tick::new(0)));
    game.run_until(3);
    assert_eq!(Attack::of(&game, slow), Attack::default());
    assert_eq!(Attack::of(&game, quick).ready_at(), Tick::new(5));
}

#[test]
fn strikes_in_one_tick_see_the_state_before_any_of_them() {
    let mut game = fight_match();
    let duelist = combatant(30, 2, 1, 5, 30).on_death(OnDeath::Stay);
    let [duelist, fighter] = arm(&mut game, [duelist, fighter()]);
    let first = Mode::spawn(&mut game, duelist, (0, 0, 0), Team::new(0), ());
    let second = Mode::spawn(&mut game, duelist, (1, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, first, second);
    Combat::attack(&mut game, second, first);

    // Both attacks start in tick 0 and strike in tick 1: each kills the other, and both stay,
    // each with the other as its attacker.
    game.run_until(2);
    for (id, other) in [(first, second), (second, first)] {
        assert_eq!(Stats::health(&game, id), 0);
        assert!(game.try_part::<Dead>(id).is_some());
        assert_eq!(Attack::of(&game, id).target(), None);
        let attackers = game.try_part::<RecentAttackers>(id).unwrap();
        let attack = RecentAttack {
            source: other,
            tick: Tick::new(1),
        };
        assert_eq!(attackers.iter().collect::<Vec<_>>(), [attack]);
    }

    // A dead unit is no target.
    let third = Mode::spawn(&mut game, fighter, (0, 0, 1), Team::new(1), ());
    Combat::attack(&mut game, third, first);
    game.run_until(3);
    assert_eq!(Attack::of(&game, third).target(), None);
}

#[test]
fn targets_are_living_enemies() {
    let mut game = fight_match();
    let prey = combatant(10, 0, 0, 1, 0);
    let [prey, stays] = arm(&mut game, [prey, prey.on_death(OnDeath::Stay)]);
    // Up at y = 9: a target all the same.
    let high = Mode::spawn(&mut game, prey, (0, 9, 3), Team::new(0), ());
    let far = Mode::spawn(&mut game, prey, (6, 0, 0), Team::new(0), ());
    let dead = Mode::spawn(&mut game, stays, (0, 0, 1), Team::new(0), Dead);
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
        let id = Mode::spawn(&mut game, prey, (x, 0, 5), Team::new(0), ());
        Units::set_blocks(&mut game, id, cases[usize::try_from(x).unwrap()].0);
        id
    });

    let enemy_at = |game: &mut Match, team: Team, target: StableId| {
        game.run_system_once(move |targets: Targets<'_, '_>| {
            targets.enemy(team, target).map(|unit| unit.pos)
        })
        .unwrap()
    };
    assert_eq!(
        enemy_at(&mut game, Team::new(1), far),
        Some(Vec3::from((6, 0, 0)))
    );
    assert_eq!(enemy_at(&mut game, Team::new(0), far), None);
    assert_eq!(enemy_at(&mut game, Team::new(1), dead), None);
    assert_eq!(
        enemy_at(&mut game, Team::new(1), high),
        Some(Vec3::from((0, 9, 3)))
    );
    for (x, (unit, (blocks, target))) in (0..).zip(units.into_iter().zip(cases)) {
        let pos = target.then_some(Vec3::from((x, 0, 5)));
        assert_eq!(enemy_at(&mut game, Team::new(1), unit), pos, "{blocks:?}");
    }
    // By the relations: team 2, friendly to team 0, may not attack its units; team 3, neutral,
    // may; team 200, past the 64 teams of before and hostile as every pair not set is, may.
    let mut relations = game.relations_mut();
    relations.set(
        Team::new(2),
        Team::new(0),
        Relation::Friendly,
        VisionShare::Shared,
    );
    relations.set(
        Team::new(3),
        Team::new(0),
        Relation::Neutral,
        VisionShare::Shared,
    );
    let related = [2, 3, 200].map(|team| enemy_at(&mut game, Team::new(team), far));
    assert_eq!(
        related,
        [
            None,
            Some(Vec3::from((6, 0, 0))),
            Some(Vec3::from((6, 0, 0)))
        ]
    );
    let entity = game.unit_entity(high);
    game.despawn(entity);
    assert_eq!(enemy_at(&mut game, Team::new(1), high), None);

    // A life at 0 that has not died yet, as `ctx.kill` leaves it until the next deaths, lives no
    // more: no target, of an area's reach neither, and not alive to a script; full again, it is
    // all three.
    let spent = Mode::spawn(&mut game, stays, (0, 0, 7), Team::new(0), ());
    let full = *game.part::<Life>(spent);
    let read = |game: &mut Match| {
        let entity = game.unit_entity(spent);
        let reached = game
            .run_system_once(move |targets: Targets<'_, '_>| targets.living_at(entity).is_some())
            .unwrap();
        let alive = Units::read(game, "of.alive", spent).unwrap();
        let enemy = enemy_at(game, Team::new(1), spent).is_some();
        [enemy, reached, alive.as_bool().unwrap()]
    };
    assert_eq!(read(&mut game), [true; 3]);
    game.insert_parts(spent, full.spent(full.current()));
    assert_eq!(read(&mut game), [false; 3]);
    assert!(game.try_part::<Dead>(spent).is_none());
    game.insert_parts(spent, full);
    assert_eq!(read(&mut game), [true; 3]);
}

#[test]
fn every_combat_type_is_state_and_restores() {
    let armed = [fighter(), combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay)];
    let mut game = fight_match();
    let [fighter, doomed] = arm(&mut game, armed);
    let fighter = Mode::spawn(&mut game, fighter, (0, 0, 0), Team::new(0), ());
    let doomed = Mode::spawn(&mut game, doomed, (1, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, fighter, doomed);
    game.run_until(3);
    assert!(game.try_part::<Dead>(doomed).is_some());

    // A restore loads the match's books first, as the packages give them: the same types and
    // weapons, in the same order.
    let mut restored = fight_match();
    arm(&mut restored, armed);
    Release::restore_into(&game, &mut restored);
    assert_eq!(
        restored.try_part::<ActionSlots>(fighter),
        game.try_part::<ActionSlots>(fighter)
    );
    assert_eq!(
        restored.try_part::<SlotTimes>(fighter),
        game.try_part::<SlotTimes>(fighter)
    );

    // The fighter's attack winds up 2 ticks: under way, it resolves in tick 2 at the soonest,
    // where it started in tick 0, and at the limit at the latest, as its weapon is ready at the
    // latest.
    let entity = restored.unit_entity(fighter);
    let past = Tick::LIMIT.get() + 1;
    let slots = |resolves_at: u64, ready_at: u64| {
        let slots = restored.part::<ActionSlots>(fighter).clone();
        let mut progress = *restored.part::<ActionProgress>(fighter);
        progress.set_attack_target(Some(doomed));
        progress.start_attack(0, Tick::new(resolves_at));
        let mut times = restored.part::<SlotTimes>(fighter).clone();
        times.cool_down(0, Tick::new(ready_at));
        let checked = slots.check(&restored, entity) && progress.check_against(&slots, &restored);
        Match::decodes(&progress) && checked && Match::decodes(&times)
    };
    assert!(slots(2, 0) && slots(Tick::LIMIT.get(), Tick::LIMIT.get()));
    assert!(!slots(1, 0));
    assert!(!slots(past, 0) && !slots(2, past));
    // An attack under way at a rank its weapon lacks is refused, not read past the weapon's
    // ranks: 0, before it is learned, and 30, past its one.
    let weapon = restored
        .part::<ActionSlots>(fighter)
        .slot(0)
        .unwrap()
        .action
        .unwrap();
    let ranked = |rank| {
        let slots = ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(rank))]);
        let mut progress = ActionProgress::default();
        progress.set_attack_target(Some(doomed));
        progress.start_attack(0, Tick::new(2));
        slots.check(&restored, entity) && progress.check_against(&slots, &restored)
    };
    assert!(ranked(1) && !ranked(0) && !ranked(30));
    // A slot of a kind the mode lacks, kind 1 of its one, is refused; slots not kind after kind
    // fail to decode.
    let lacks = ActionSlots::new([(weapon, SlotKind::new(1), Rank::new(1))]);
    assert!(!lacks.check(&restored, entity));
    let of_kind = |kind| slot_of_kind(weapon, kind);
    let decodes =
        |slots: Vec<ActionSlot>| Binary::decode::<ActionSlots>(&Binary::encode(&slots)).is_ok();
    assert!(decodes(vec![of_kind(0), of_kind(1)]));
    assert!(!decodes(vec![of_kind(1), of_kind(0)]));
}

/// The one slot of `weapon` in `kind`, at rank 1.
fn slot_of_kind(weapon: ActionId, kind: u8) -> ActionSlot {
    let slots = ActionSlots::new([(weapon, SlotKind::new(kind), Rank::new(1))]);
    slots.slot(0).unwrap()
}

#[test]
fn stats_out_of_their_limits_are_refused() {
    let mana = PoolId::new(1).unwrap();
    let pools = |max| Pools::new([(mana, max)]);
    assert_eq!(pools(Num::ZERO), None);
    assert_eq!(
        pools(Num::EPSILON).and_then(|pools| pools.current(mana)),
        Some(Num::EPSILON)
    );

    // A snapshot's values pass the same limits, and carry less than a bit, at 30 ticks a second
    // fewer than 30 parts, between its ends alone; a life the same.
    let meter = |current: i64, max: i64, carry: u32| (Num::int(current), Num::int(max), carry);
    let carrying = |place: usize, current, max, carry| {
        let mut meters = [None; PoolId::LIMIT];
        meters[place] = Some(meter(current, max, carry));
        let bytes = Binary::encode(&meters);
        Binary::decode::<Pools>(&bytes).ok()
    };
    let living = |current, max, carry| {
        let bytes = Binary::encode(&meter(current, max, carry));
        Binary::decode::<Life>(&bytes).ok()
    };
    for (current, max, reads) in [(0, 1, true), (1, 1, true), (-1, 1, false), (2, 1, false)] {
        assert_eq!(
            carrying(1, current, max, 0).is_some(),
            reads,
            "{current} of {max}"
        );
        assert_eq!(
            living(current, max, 0).is_some(),
            reads,
            "{current} of {max}"
        );
    }
    assert_eq!((carrying(1, 0, 0, 0), living(0, 0, 0)), (None, None));
    let mut game = fight_match();
    let [fighter] = arm(&mut game, [fighter()]);
    let id = Mode::spawn(&mut game, fighter, (0, 0, 0), Team::new(0), ());
    let entity = game.unit_entity(id);
    let restores = |current, carry| {
        let pools = carrying(1, current, 2, carry).unwrap();
        let life = living(current, 2, carry).unwrap();
        let checks = pools.check(&game, entity);
        assert_eq!(
            life.check(&game, entity),
            checks,
            "{current} carrying {carry}"
        );
        checks
    };
    assert!(restores(1, 29) && restores(2, 0));
    assert!(!restores(1, 30) && !restores(2, 1) && !restores(0, 1));
    // The life pool's place, 0, holds no meter of the pools, as the life holds it.
    let at_life = carrying(0, 1, 2, 0).unwrap();
    assert!(!at_life.check(&game, entity));
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
        let mut game = fight_match();
        if let Some(ticks) = window {
            game.resource_mut::<CombatBook>()
                .set_assist_window(Some(Ticks::new(ticks)));
        }
        let team = Team::new(0);
        let victim = combatant(130, 0, 0, 1, 0);
        let armed = [fighter(), slow, victim.on_death(OnDeath::Stay), victim];
        let [fighter, slow, hero, creep] = arm(&mut game, armed);
        let [first, killer] = [4, 6].map(|x| Mode::spawn(&mut game, fighter, (x, 0, 0), team, ()));
        let slow_one = Mode::spawn(&mut game, slow, (5, 0, 0), team, ());
        let owner = Owner::new(PlayerSlot::new(3));
        let hero = Mode::spawn(&mut game, hero, (5, 0, 1), Team::new(1), owner);
        let creep = Mode::spawn(&mut game, creep, (5, 0, -1), Team::new(1), ());
        let [creep_first, creep_killer] =
            [4, 6].map(|x| Mode::spawn(&mut game, fighter, (x, 0, -1), team, ()));
        let creep_slow = Mode::spawn(&mut game, slow, (5, 0, -1), team, ());
        for (attacker, target) in [
            (first, hero),
            (killer, hero),
            (slow_one, hero),
            (creep_first, creep),
            (creep_killer, creep),
            (creep_slow, creep),
        ] {
            Combat::attack(&mut game, attacker, target);
        }
        game.run_until(7);
        assert_eq!(Stats::health(&game, hero), 40, "{window:?}");
        game.run_until(8);
        // Each death holds its tick, 7, and the unit as it was: its team, its owner, and whether
        // it was its owner's avatar, a record that outlives the creep.
        let deaths = game.resource::<Deaths>();
        assert_eq!(deaths.tick(), Tick::new(7));
        let seen: Vec<_> = deaths
            .iter()
            .map(|death| (death.fallen, death.killer, death.assisters.to_vec()))
            .collect();
        // The hero is owned, and no avatar: its type has no avatar tag.
        let fallen = |id, owner| Fallen {
            id,
            team: Some(Team::new(1)),
            owner,
            avatar: false,
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
        assert!(game.try_part::<Dead>(hero).is_some());
        assert!(game.try_part::<Life>(creep).is_none());
    }

    // The hero comes back at the start of its respawn tick, 10, at its spawn point with full
    // health and no attacker on record; the attackers dropped it when it died.
    let mut game = fight_match();
    let stays = combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay);
    let [fighter, stays] = arm(&mut game, [fighter(), stays]);
    let attacker = Mode::spawn(&mut game, fighter, (4, 0, 0), Team::new(0), ());
    let spawn = SpawnPoint::new(Vec3::from((-3, 0, 2)));
    let hero = Mode::spawn(&mut game, stays, (5, 0, 0), Team::new(1), spawn);
    Combat::attack(&mut game, attacker, hero);
    game.run_until(3);
    assert!(game.try_part::<Dead>(hero).is_some());
    // Dead, it carries the engine's `dead` tag, which its derived tags take from its mark.
    let tagged_dead = |game: &Match| {
        let tags = game.try_part::<UnitTags>(hero).unwrap();
        tags.tags.contains(Dead::TAG.tag())
    };
    assert!(tagged_dead(&game));
    let at_tick = Tick::new(10);
    game.insert_parts(hero, Respawn { at: at_tick });
    game.run_until(10);
    assert!(game.try_part::<Dead>(hero).is_some());
    let attackers = |game: &Match| {
        game.try_part::<RecentAttackers>(hero)
            .map(|r| r.iter().count())
    };
    assert_eq!(attackers(&game), Some(1));
    game.run_until(11);
    assert!(game.try_part::<Dead>(hero).is_none() && game.try_part::<Respawn>(hero).is_none());
    assert!(!tagged_dead(&game));
    assert_eq!(
        game.try_part::<Position>(hero).map(|pos| pos.get()),
        Some(Vec3::from((-3, 0, 2)))
    );
    assert_eq!(Stats::health(&game, hero), 30);
    assert_eq!(attackers(&game), Some(0));
    assert_eq!(Attack::of(&game, attacker).target(), None);
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
    let book = StatBook::new(&rules, Num::int(10));
    let health = DeclaredName::new("health").unwrap();
    let combat = ModeCombatData {
        life: Some(health.clone()),
        leech: Some(LeechData {
            attack: Some(attack),
            other: Some(other),
        }),
        heal_scale: Some(heal),
        ..ModeCombatData::default()
    };
    let pools = Names::<PoolId>::from([health.as_str()]);
    let life = combat.life_pool(&pools).unwrap();
    let bound = CombatBook::new(&combat, Match::TEST_RATE, |stat| {
        book.named(stat).ok_or_else(|| stat.clone())
    });
    world.insert_resource(bound.unwrap());
    Stats::load_book(world, book);
    world.resource_mut::<PoolBook>().set_life(life);
}

/// A modifier of no stats, tags or script, as each shield the shield test gives is.
fn shield_data() -> ModifierData {
    ModifierData {
        ..ModifierData::default()
    }
}

/// A part of no meaning, which moves the unit that takes it to another archetype.
#[derive(Component, Debug)]
struct Apart;

const ATTACK: DamageCause = DamageCause::Attack {
    roll: Num::ZERO,
    rank: Rank::FIRST,
};

#[test]
fn the_pass_deals_damage_in_its_order_and_credits_the_kill() {
    let mut game = fight_match();
    game.resource_mut::<CombatBook>()
        .set_assist_window(Some(Ticks::new(10)));
    let [dummy, stays] = arm(&mut game, [dummy(), dummy().on_death(OnDeath::Stay)]);
    let a = Mode::spawn(&mut game, dummy, (0, 0, 0), Team::new(0), ());
    let b = Mode::spawn(&mut game, dummy, (1, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, stays, (2, 0, 0), Team::new(1), ());
    assert!(a < b);
    // Queued from b, then a, then none: none's 30 goes first, 100 to 70, then a's 60, to 10, then
    // b's 60, to 0. b killed it, and a, which damaged it this tick, assisted.
    Combat::damage(&mut game, Some(b), target, 60, ATTACK);
    Combat::damage(&mut game, Some(a), target, 60, DamageCause::Effect);
    Combat::damage(&mut game, None, target, 30, DamageCause::Effect);
    game.run_until(1);
    assert_eq!(Stats::health(&game, target), 0);
    let deaths = game.resource::<Deaths>();
    let died: Vec<_> = deaths
        .iter()
        .map(|death| (death.fallen.id, death.killer, death.assisters.to_vec()))
        .collect();
    assert_eq!(died, [(target, Some(b), vec![a])]);

    // At zero health it takes nothing more, records no attacker, and heals nothing.
    let c = Mode::spawn(&mut game, dummy, (3, 0, 0), Team::new(0), ());
    Combat::damage(&mut game, Some(c), target, 10, ATTACK);
    game.run_until(2);
    Combat::heal_unit(&mut game, target, 50);
    assert_eq!(Stats::health(&game, target), 0);
    let attackers = game.try_part::<RecentAttackers>(target).unwrap();
    assert!(attackers.iter().all(|attack| attack.source != c));
    assert_eq!(game.resource::<Deaths>().iter().count(), 0);

    // An invulnerable unit takes nothing and records no attacker; untargetable, it takes all.
    let guarded = Mode::spawn(&mut game, dummy, (4, 0, 0), Team::new(1), ());
    let hidden = Mode::spawn(&mut game, dummy, (5, 0, 0), Team::new(1), ());
    Units::set_blocks(&mut game, guarded, &[Block::Target, Block::Damage]);
    Units::set_blocks(&mut game, hidden, &[Block::Target]);
    Combat::damage(&mut game, Some(c), guarded, 10, DamageCause::Effect);
    Combat::damage(&mut game, Some(c), hidden, 10, DamageCause::Effect);
    game.run_until(3);
    assert_eq!(
        (Stats::health(&game, guarded), Stats::health(&game, hidden)),
        (100, 90)
    );
    let attackers = game.try_part::<RecentAttackers>(guarded).unwrap();
    assert_eq!(attackers.iter().count(), 0);
    // Two units at zero health that no strike took there die with no killer, in stable-id order,
    // though the lower id's part, inserted after the higher id's spawn, puts it in a later
    // archetype, which a query meets after.
    let first = Mode::spawn(&mut game, dummy, (6, 0, 0), Team::new(0), ());
    let second = Mode::spawn(&mut game, dummy, (7, 0, 0), Team::new(0), ());
    assert!(first < second);
    for id in [second, first] {
        let spent = game.part::<Life>(id).spent(Num::int(100));
        game.insert_parts(id, spent);
    }
    game.insert_parts(first, Apart);
    game.run_until(4);
    let died: Vec<_> = game
        .resource::<Deaths>()
        .iter()
        .map(|death| (death.fallen.id, death.killer))
        .collect();
    assert_eq!(died, [(first, None), (second, None)]);
}

#[test]
fn damage_after_the_pass_waits_for_the_next_and_a_life_at_zero_acts_no_more() {
    // A pass after the Mode stage stands for a mode's hook in tick 1, after the tick's pass: it
    // queues 30 damage from none to the target, and takes the attacker's life to 0, as
    // `ctx.kill` does. The attacker's windup, from tick 0, ends in tick 2.
    let mut game = fight_match();
    let armed = [
        fighter().on_death(OnDeath::Stay),
        dummy().on_death(OnDeath::Stay),
    ];
    let [fighter, stays] = arm(&mut game, armed);
    let attacker = Mode::spawn(&mut game, fighter, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, stays, (1, 0, 0), Team::new(1), ());
    Combat::attack(&mut game, attacker, target);
    let hook = move |world: &mut World| {
        if world.resource::<SimTick>().start() != Tick::new(1) {
            return;
        }
        let damage = Damage {
            source: None,
            target,
            amount: Num::int(30),
            kind: DamageKind::new(0),
            cause: DamageCause::Effect,
            action: None,
            depth: 0,
            hit: None,
        };
        DamagePass::queue(world, PassEntry::Damage(damage));
        let entity = world.entity_of(attacker).unwrap();
        let mut life = world.get_mut::<Life>(entity).unwrap();
        let all = life.current();
        life.take(all);
    };
    let hook = hook
        .in_set(SimEdge::After(SimSet::Mode))
        .after(StatsSet::Refresh(SimEdge::After(SimSet::Mode)));
    game.resource_mut::<Schedules>()
        .add_systems(SimUpdate, hook);
    game.run_until(2);
    assert_eq!(Stats::health(&game, target), 100);
    assert!(game.try_part::<Dead>(attacker).is_none());

    // In tick 2 the pass deals the 30 first, to 70; the attacker, at 0, strikes nothing, where
    // its 30 would leave 40, and dies with no killer. A match restored at the boundary between
    // the ticks holds the damage still to come, and plays tick 2 alike.
    let mut restored = fight_match();
    arm(&mut restored, armed);
    Release::restore_into(&game, &mut restored);
    for game in [&mut game, &mut restored] {
        game.run_until(3);
        assert_eq!(Stats::health(game, target), 70);
        assert!(game.try_part::<Dead>(attacker).is_some());
        let died: Vec<_> = game
            .resource::<Deaths>()
            .iter()
            .map(|death| (death.fallen.id, death.killer))
            .collect();
        assert_eq!(died, [(attacker, None)]);
    }
}

#[test]
fn shields_absorb_soonest_end_first_and_vamps_heal_from_health_taken() {
    let mut game = fight_match();
    load_damage_stats(&mut game);
    let half = Num::HALF;
    // The source heals halved, with life steal 0.5 and spell vamp 0.25, and has a second pool,
    // of 100; another heals at the largest scale.
    let [dummy, source, scaled] = [dummy(), dummy(), dummy()].map(|armed| armed.load(&mut game));
    Stats::give_type_stats(&mut game, source, &[-half, half, Num::ONE / 4]);
    Stats::give_type_stats(&mut game, scaled, &[Num::MAX, half, Num::ONE / 4]);
    let mana = PoolId::new(1).unwrap();
    Units::give_type(
        &mut game,
        source,
        Pools::new([(mana, Num::int(100))]).unwrap(),
    );
    let source = Mode::spawn(&mut game, source, (0, 0, 0), Team::new(0), ());
    let target = Mode::spawn(&mut game, dummy, (1, 0, 0), Team::new(1), ());
    let scaled = Mode::spawn(&mut game, scaled, (0, 0, 2), Team::new(0), ());
    // The source at 40 of 100.
    let entity = game.unit_entity(source);
    let mut life = game.get_mut::<Life>(entity).unwrap();
    life.take(Num::int(60));
    let shield = |id: u16, until: Option<u64>, amount: i64| NewInstance {
        lifetime: Lifetime::applied(until.map_or(Ends::Never, |until| Ends::At(Tick::new(until)))),
        shield: Some(Num::int(amount)),
        ..NewInstance::bare(ModifierId::new(id), None)
    };
    for name in ["first", "second", "third"] {
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, &shield_data(), None);
    }
    let shields = [
        shield(0, None, 100),
        shield(1, Some(50), 20),
        shield(2, Some(30), 10),
    ]
    .map(Application::refresh);
    game.insert_parts(target, Modifiers::bundle(shields));
    let shields = |game: &Match| {
        let clocks = game.try_part::<ModifierClocks>(target).unwrap();
        let held = game.try_part::<Modifiers>(target).unwrap().len();
        // Every shield held has some left, which the damage pass sees before it absorbs.
        assert_eq!(clocks.shielded(), held > 0);
        (0..held)
            .map(|at| clocks.shield(at).unwrap())
            .collect::<Vec<_>>()
    };

    // 35: the shield that ends in tick 30 spends its 10, the one of tick 50 its 20, and the one
    // with no end 5 of its 100; health takes nothing, and the source heals nothing. The target's
    // life, which nothing took from, is not written.
    let life_changed = |game: &Match| {
        let unit = game.entity(game.unit_entity(target));
        unit.get_ref::<Life>().unwrap().last_changed()
    };
    let before = life_changed(&game);
    Combat::damage(&mut game, Some(source), target, 35, ATTACK);
    game.run_until(1);
    assert_eq!(shields(&game), [Num::int(95)]);
    assert_eq!(game.part::<Life>(target).current(), Num::int(100));
    assert_eq!(game.part::<Life>(source).current(), Num::int(40));
    assert_eq!(life_changed(&game), before);
    // An attack of 100: the last shield's 95, then 5 off health, which life steals 5 × 0.5,
    // halved: 1.25. Then a spell of 40, all off health: 40 × 0.25, halved, 5. 40 + 1.25 + 5.
    Combat::damage(&mut game, Some(source), target, 100, ATTACK);
    Combat::damage(&mut game, Some(source), target, 40, DamageCause::Effect);
    game.run_until(2);
    assert_eq!(shields(&game), []);
    assert_eq!(game.part::<Life>(target).current(), Num::int(55));
    assert_eq!(game.part::<Life>(source).current(), Num::int(185) / 4);
    // A heal of 10 is halved too; one past the maximum stops at it.
    Combat::heal_unit(&mut game, source, 10);
    assert_eq!(game.part::<Life>(source).current(), Num::int(205) / 4);
    Combat::heal_unit(&mut game, source, 1000);
    assert_eq!(game.part::<Life>(source).current(), Num::int(100));
    // A heal scale at the largest number, one past which no number holds, scales as the largest:
    // a heal of 1 from 40 fills the pool.
    game.part_mut::<Life>(scaled).take(Num::int(60));
    Combat::heal_unit(&mut game, scaled, Num::ONE);
    assert_eq!(game.part::<Life>(scaled).current(), Num::int(100));
    // Leech is dealt next, before the rest of the queue: at 10, an attack of 20 steals 20 × 0.5,
    // halved, 5, before the 12 of a foe, whose higher id sorts it after: 10 + 5 − 12. Dealt at
    // the end of the queue, it would come after the 12, which kills.
    let foe = Mode::spawn(&mut game, dummy, (2, 0, 0), Team::new(1), ());
    assert!(source < foe);
    game.part_mut::<Life>(source).take(Num::int(90));
    Combat::damage(&mut game, Some(foe), source, 12, DamageCause::Effect);
    Combat::damage(&mut game, Some(source), target, 20, ATTACK);
    game.run_until(3);
    assert_eq!(game.part::<Life>(source).current(), Num::int(3));
    assert_eq!(game.part::<Life>(target).current(), Num::int(35));
    // Without the bindings the same stats do nothing: at 50, an attack of 10 heals the source
    // nothing, and a heal of 10 is whole.
    Combat::bind_life(&mut game, PoolId::FIRST);
    let entity = game.unit_entity(source);
    let mut life = game.get_mut::<Life>(entity).unwrap();
    life.add(Num::int(47));
    Combat::damage(&mut game, Some(source), target, 10, ATTACK);
    game.run_until(4);
    assert_eq!(game.part::<Life>(target).current(), Num::int(25));
    assert_eq!(game.part::<Life>(source).current(), Num::int(50));
    Combat::heal_unit(&mut game, source, 10);
    assert_eq!(game.part::<Life>(source).current(), Num::int(60));
    // A restore reaches the pool it names, unscaled: a second pool at 50 of 100 takes 20 more,
    // and the life pool keeps its 60; one of the life pool's id reaches the life, which takes 15
    // more, unscaled, to 75, and the second pool keeps its 70.
    game.part_mut::<Pools>(source).take(mana, Num::int(50));
    Stats::restore(&mut game, source, mana, Num::int(20));
    let held = |game: &Match| {
        let mana = game.try_part::<Pools>(source).unwrap().current(mana);
        (game.part::<Life>(source).current(), mana)
    };
    assert_eq!(held(&game), (Num::int(60), Some(Num::int(70))));
    Stats::restore(&mut game, source, PoolId::FIRST, Num::int(15));
    assert_eq!(held(&game), (Num::int(75), Some(Num::int(70))));
}

#[test]
fn an_attack_draws_its_roll_once_as_its_windup_ends_from_the_seed() {
    let mut game = fight_match();
    let [dummy, attacker] = arm(&mut game, [dummy(), combatant(100, 2, 0, 5, 30)]);
    let dummy = Mode::spawn(&mut game, dummy, (0, 0, 0), Team::new(1), ());
    let attackers: Vec<_> = (0..16)
        .map(|_| Mode::spawn(&mut game, attacker, (1, 0, 0), Team::new(0), ()))
        .collect();
    // Each attacker's windup ends in tick 0; the roll of each, by the attackers' order.
    let rolls = |game: &mut Match| {
        for &attacker in &attackers {
            let mut progress = game.part_mut::<ActionProgress>(attacker);
            progress.set_attack_target(Some(dummy));
            progress.start_attack(0, Tick::new(0));
        }
        game.resource_mut::<PassQueue>().clear();
        Combat::strike_now(game);
        let queue = game.resource::<PassQueue>();
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
    let draw = |seed: u8, id: &StableId| {
        Rng::new(
            &SegmentSeed::new([seed; 32]),
            ROLL_STREAM,
            id.get(),
            Tick::ZERO,
        )
    };
    let expected = |seed: u8| {
        let roll = |id| draw(seed, id).fraction();
        attackers.iter().map(roll).collect::<Vec<_>>()
    };
    let first = rolls(&mut game);
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
    let rolled = |id| draw(0, id).chance(half);
    assert_eq!(crits, attackers.iter().map(rolled).collect::<Vec<_>>());
    assert!(crits.contains(&true) && crits.contains(&false));
    game.insert_resource(SimRng::new(SegmentSeed::new([1; 32])));
    let second = rolls(&mut game);
    assert_eq!(second, expected(1));
    assert_ne!(first, second);
}

#[test]
fn a_predicting_clients_combat_reads_only_what_it_holds() {
    // A client predicts its fighter and its ally, and holds two enemies as the server sent them:
    // its fighter attacks the near enemy, which attacks it back, and the far one stands. The
    // client strikes only what it predicts, and reads of the enemies only what reaches it.
    Release::sync_held(&[Capability::Stats, Capability::Combat], 12, |game| {
        Combat::load_weapon_stats(game);
        let [fighter, dummy] = arm(game, [fighter(), dummy()]);
        let own = Mode::spawn(game, fighter, (0, 0, 0), Team::new(0), ());
        Mode::spawn(game, dummy, (0, 0, 1), Team::new(0), ());
        let enemy = Mode::spawn(game, fighter, (1, 0, 0), Team::new(1), Unpredicted);
        Mode::spawn(game, dummy, (9, 0, 0), Team::new(1), Unpredicted);
        Combat::attack(game, own, enemy);
        Combat::attack(game, enemy, own);
    });
}

#[test]
#[should_panic(expected = "the units the client predicts end alike")]
fn a_read_of_what_the_client_lacks_fails_the_sync_test() {
    // A system moves the client's fighter when the enemy it holds as the server sent it has
    // attackers on record, `RecentAttackers`, which no client receives of such a unit: with the
    // enemy's state whole the fighter moves, held as sent it stands, and the two differ.
    Release::sync_held(&[Capability::Stats, Capability::Combat], 1, |game| {
        Combat::load_weapon_stats(game);
        let [fighter] = arm(game, [fighter()]);
        let own = Mode::spawn(game, fighter, (0, 0, 0), Team::new(0), ());
        let enemy = Mode::spawn(game, fighter, (5, 0, 0), Team::new(1), Unpredicted);
        let peek = move |world: &mut World| {
            let enemy = world.entity_of(enemy).unwrap();
            if world.get::<RecentAttackers>(enemy).is_some() {
                let own = world.entity_of(own).unwrap();
                *world.get_mut::<Position>(own).unwrap() = Position::from((1, 0, 0));
            }
        };
        let peek = peek.in_set(SimEdge::After(SimSet::Mode));
        let after = peek.after(StatsSet::Refresh(SimEdge::After(SimSet::Mode)));
        game.resource_mut::<Schedules>()
            .add_systems(SimUpdate, after);
    });
}

mod slow_weapons;
mod turrets;
