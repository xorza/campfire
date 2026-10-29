use std::collections::BTreeMap;
use std::num::{NonZeroI64, NonZeroU16, NonZeroU32};

use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use bevy_ecs::observer::On;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{ResMut, RunSystemOnce};
use bevy_ecs::world::World;
use campfire_actions::internals::LooseDues;
use campfire_actions::{
    ActionProgress, ActionRange, ActionSlot, ActionSlots, Actions, AttackTarget, InProgress,
    ProgressDues, SlotKind, SlotTimes, Targets, TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_codec::Binary;
use campfire_combat::internals::{Armed, Attack, TestWeapon};
use campfire_combat::*;
use campfire_common::{DeclaredName, PlayerSlot, SegmentSeed, Tick, Ticks};
use campfire_declarations::internals::BookLoad;
use campfire_declarations::{ById, Role};
use campfire_geometry::Metric;
use campfire_math::{Num, Rng, Vec3};
use campfire_names::{Filter, MatchNames};
use campfire_navigation::{FlightData, Navigation};
use campfire_players::{Players, PlayersWorld, ResourceAmount};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{
    Capability, EntityIndex, MatchRate, Position, SimComponent, SimEdgeSystems, SimRng, SimSystems,
    SimTick, SimWorld, StableId, Unpredicted,
};
use campfire_stats::{
    Application, Ends, Lifetime, ModifierClocks, ModifierData, Modifiers, NewInstance, PoolBook,
    Pools, StatBook, StatRule, Stats, StatsSystems,
};
use campfire_units::internals::Spawn;
use campfire_units::{BodyForm, BodyShape, Died, Layer, MapSpace, Teleported};
use campfire_units::{
    Dead, Deaths, Fallen, Life, MapBook, Owner, PoolCost, PoolId, SpawnPoint, StatId, UnitTypeCore,
    Units, UnitsWorld,
};
use campfire_values::TagSet;
use campfire_values::{
    ActionId, Allegiance, Block, DamageKind, FilterData, ModifierId, NameId, Names, PackageIndex,
    Rank, Relation, RelationSet, ResourceId, Stat, Team, TypeScope, UnitTags, UnitType,
    VisionShare,
};

use crate::combatant;

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
    let mut game = Match::builder(&[Capability::Stats, Capability::Combat]).build();
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
fn load_types<const N: usize>(game: &mut Match, armed: [Armed; N]) -> [UnitType; N] {
    armed.map(|armed| armed.load(game))
}

#[test]
fn an_attack_winds_up_and_strikes_each_period() {
    let mut game = fight_match();
    let [fighter, dummy] = load_types(&mut game, [fighter(), dummy()]);
    let fighter = Spawn::of(fighter, (4, 0, 0)).place(&mut game);
    let dummy = Spawn::of(dummy, (6, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
    assert!(game.try_part::<Dead>(dummy).is_some());
    assert_eq!(Attack::of(&game, fighter).target(), Some(dummy));
    // The next tick finds the target dead and drops it.
    game.run_until(19);
    assert_eq!(Attack::of(&game, fighter).target(), None);
    // The dummy, dead in tick 17, decays at the end of tick 26, 9 ticks on.
    game.run_until(26);
    assert!(game.try_part::<Dead>(dummy).is_some());
    game.run_until(27);
    assert!(game.try_part::<Life>(dummy).is_none());

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
/// 3 shots that reloads in 21 ticks, 700 ms, and of `pre_attack`, at (4, 0), and a dummy of team
/// 1 with `held` at (6, 0), which the rifle attacks; the two.
fn rifle_fight(
    game: &mut Match,
    held: impl Bundle<Effect: NoBundleEffect>,
    pre_attack: Option<PreAttackRule>,
) -> [StableId; 2] {
    let [dummy] = load_types(game, [dummy()]);
    let weapon = TestWeapon {
        rate: Rate::Period(Box::new([Ticks::new(3)])),
        damage: WeaponDamage::Amount(Box::new([Num::int(5)])),
        clip: Some(ClipRule {
            shots: NonZeroU16::new(3).unwrap(),
            reload: Ticks::new(21),
            refill: ClipRefill::Time,
        }),
        pre_attack,
        ..TestWeapon::new(
            Filter::of_match_relations(game, RelationSet::Enemies),
            ActionRange::meters(Num::int(2)),
            Ticks::ZERO,
        )
    };
    let rifle = Combat::add_weapon(game, weapon);
    let slots = ActionSlots::new([(rifle, SlotKind::new(0), Rank::new(1))]);
    let shooter = wielder(game, slots, &[]);
    let shooter = Spawn::of(shooter, (4, 0, 0)).place(game);
    let dummy = Spawn::of(dummy, (6, 0, 0))
        .team(Team::new(1))
        .parts(held)
        .place(game);
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
    let fight = rifle_fight(&mut game, (), None);
    let [ready, hits] = rifle_ticks(&mut game, fight);
    assert_eq!(ready, readies);
    assert_eq!(hits, [0, 3, 6, 27, 30, 33, 54]);
    assert_eq!(Stats::health(&game, fight[1]), 65);
    // A predicting client, which holds the dummy as the server sent it and deals no damage,
    // counts the clip as the server does, with the count the server sends of its own shooter:
    // its rifle is ready again in the same ticks.
    let mut client = Match::builder(&[Capability::Stats, Capability::Combat])
        .role(Role::Client)
        .build();
    Combat::load_weapon_stats(&mut client);
    let fight = rifle_fight(&mut client, Unpredicted, None);
    let [ready, hits] = rifle_ticks(&mut client, fight);
    assert_eq!(ready, readies);
    assert_eq!(hits, Vec::<u64>::new());
}

#[test]
fn a_pre_attack_runs_before_a_start_and_starts_again_as_its_each_says() {
    // The rifle strikes in the tick its pre-attack ends once it is ready, and the pre-attack
    // starts in the tick after a strike, so it runs through the period; one that ends before the
    // rifle is ready starts again in the tick after. With one of 2 ticks each shot, the strikes
    // come at 2, 5 and 8, as 2 + 3 + 3; the clip then reloads to 29, and the pre-attacks begun in
    // 9, 12, … 27 end in 29: 29, 32, 35. One of 4 ticks each shot outlasts the period: 4, then
    // 5 + 4 = 9 and 14, empty, ready at 35; those begun in 15, 20, 25, 30 and 35 end in 19, 24,
    // 29, 34 and 39: 39, 40 + 4 = 44, 49. Of 4 ticks each target, the first shot alone waits: 4,
    // 7, 10, then 31, 34, 37 after the reload. Of each clip, the pre-attack starts again as the
    // clip empties, as its reload loads it: of 4 ticks, 4, 7, 10, and those begun in 11, 16, 21,
    // 26 and 31 end in 35: 35, 38, 41; of 2 ticks, as of each shot, 2, 5, 8, 29, 32, 35.
    let pre = |ticks: u64, each| Some(PreAttackRule::new(Box::new([Ticks::new(ticks)]), each));
    let hits = |pre_attack| {
        let mut game = fight_match();
        let fight = rifle_fight(&mut game, (), pre_attack);
        rifle_ticks(&mut game, fight)[1].clone()
    };
    assert_eq!(hits(pre(2, PreAttackEach::Shot)), [2, 5, 8, 29, 32, 35]);
    assert_eq!(hits(pre(4, PreAttackEach::Shot)), [4, 9, 14, 39, 44, 49]);
    assert_eq!(hits(pre(4, PreAttackEach::Target)), [4, 7, 10, 31, 34, 37]);
    assert_eq!(hits(pre(4, PreAttackEach::Clip)), [4, 7, 10, 35, 38, 41]);
    assert_eq!(hits(pre(2, PreAttackEach::Clip)), [2, 5, 8, 29, 32, 35]);

    // A loss of reach ends it: the dummy steps out in tick 2 and back in tick 3, so the
    // pre-attack the shooter began in tick 0 starts again in tick 3, done in 7.
    let mut game = fight_match();
    let [shooter, dummy] = rifle_fight(&mut game, (), pre(4, PreAttackEach::Shot));
    game.run_until(2);
    let held = |game: &Match| {
        let progress = game.try_part::<ActionProgress>(shooter).unwrap();
        progress
            .pre_attack()
            .map(|pre| (pre.done_at.get(), pre.spent))
    };
    assert_eq!(held(&game), Some((4, false)));
    *game.part_mut::<Position>(dummy) = Position::from((20, 0, 0));
    game.run_until(3);
    assert_eq!(held(&game), None);
    *game.part_mut::<Position>(dummy) = Position::from((6, 0, 0));
    game.run_until(4);
    assert_eq!(held(&game), Some((7, false)));
    game.run_until(8);
    assert_eq!(Stats::health(&game, dummy), 95);
    assert_eq!(held(&game), Some((7, true)));
}

#[test]
fn every_unit_with_slots_counts_its_clips_from_its_spawn_or_a_grant() {
    // On a server, the shooter and the dummy, of a weapon of no clip, spawn with their counts,
    // empty. A unit of no slots holds none, and one once a grant gives it the rifle's.
    let mut game = fight_match();
    let bare = Units::load_next_type(&mut game, &UnitTypeCore::default());
    let [shooter, dummy] = rifle_fight(&mut game, (), None);
    let counts = |game: &Match, id| game.try_part::<ClipShots>(id).cloned();
    assert_eq!(counts(&game, shooter), Some(ClipShots::default()));
    assert_eq!(counts(&game, dummy), Some(ClipShots::default()));
    let rifle = game
        .part::<ActionSlots>(shooter)
        .slot(0)
        .unwrap()
        .action
        .unwrap();
    let bare = Spawn::of(bare, (9, 0, 0)).place(&mut game);
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
        MapBook::amend(&mut game, |_, map| map.space = MapSpace::from(metric));
        let types = [fighter(), dummy()].map(|armed| armed.load(&mut game));
        if bodies {
            for unit_type in types {
                Units::give_type(
                    &mut game,
                    unit_type,
                    BodyForm::new(BodyShape::circle(half).unwrap(), Layer::FIRST),
                );
            }
        }
        let fighter = Spawn::of(types[0], (0, 0, 0)).place(&mut game);
        let dummy_at = Vec3::new(dummy_x, Num::int(dummy_y), Num::ZERO);
        let dummy = Spawn::of(types[1], dummy_at)
            .team(Team::new(1))
            .place(&mut game);
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
fn a_unit_attacks_with_its_ready_weapon_of_the_most_damage_whose_filter_selects_the_target() {
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
        let aim = game
            .resource::<MatchNames>()
            .resolve_filter(&FilterData::parse(aim).unwrap())
            .unwrap();
        let weapon = TestWeapon {
            damage,
            ..TestWeapon::new(
                aim,
                ActionRange::meters(Num::int(range)),
                Ticks::new(windup),
            )
        };
        Combat::add_weapon(game, weapon)
    };
    let ground = weapon(
        &mut game,
        "enemies:ground",
        1,
        0,
        WeaponDamage::Stat(StatId::new(1)),
    );
    let air = weapon(
        &mut game,
        "enemies:air",
        5,
        1,
        WeaponDamage::Stat(StatId::new(2)),
    );
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
    let id = Spawn::of(rts, (0, 0, 0)).place(&mut game);
    let [ground_at, air_at, structure_at, hover_at] = [1, 4, 1, 1];
    let targets = [
        (types[0], ground_at),
        (types[1], air_at),
        (types[2], structure_at),
        (hover, hover_at),
    ]
    .map(|(unit_type, x)| {
        Spawn::of(unit_type, (x, 0, 0))
            .team(Team::new(1))
            .place(&mut game)
    });
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

    // Both weapons select a hovering unit, 1 m off, both ready: the air one deals more, 25 over
    // 10, and starts in tick 8 to strike in tick 9.
    Combat::attack(&mut game, id, hovering);
    game.run_until(9);
    assert_eq!(underway(&game), Some(1));
    game.run_until(10);
    assert_eq!(healths(&game), [90, 75, 100, 75]);
    // The air one cools until tick 8 + 5 = 13: the ground one, ready since tick 7, strikes in
    // tick 10 itself, for 10. Both cooling, the air one deals more, starts once ready in tick
    // 13, and strikes in tick 14.
    game.run_until(11);
    assert_eq!(healths(&game), [90, 75, 100, 65]);
    game.run_until(13);
    assert_eq!(underway(&game), None);
    game.run_until(14);
    assert_eq!(underway(&game), Some(1));
    game.run_until(15);
    assert_eq!(healths(&game), [90, 75, 100, 40]);

    // A weapon not learned is none: a unit 1 m off the ground unit, its ground weapon at rank 0,
    // starts nothing at it.
    let second = Spawn::of(second, (1, 0, 1)).place(&mut game);
    Combat::attack(&mut game, second, ground_unit);
    game.run_until(17);
    let progress = game.try_part::<ActionProgress>(second).unwrap();
    assert_eq!((progress.weapon(), progress.in_progress()), (None, None));
    assert_eq!(healths(&game)[0], 90);
}

#[test]
fn a_preferred_weapon_comes_first_cooling_or_not_then_the_most_damage_its_armor_takes() {
    // A unit of three weapons of 5 m, no windup and a period of 5 ticks, at every enemy: in slot
    // 0 a cannon of 10 of the first kind, preferring infantry; in slot 1 a missile of 30 of the
    // second kind, preferring vehicles; in slot 2 a rifle of 10 of the first kind.
    let mut game = fight_match();
    let [infantry, vehicle] = ["infantry", "vehicle"].map(|tag| {
        let data = UnitTypeCore::tagged(&[tag]);
        Units::load_type(&mut game, TypeScope::Mode, tag, &data)
    });
    let names = game.resource::<MatchNames>();
    let filter = |text| {
        names
            .resolve_filter(&FilterData::parse(text).unwrap())
            .unwrap()
    };
    let [cannon_prefers, missile_prefers] = ["all:infantry", "all:vehicle"].map(filter);
    let enemies = filter("enemies");
    let mut weapon = |amount, kind, prefers| {
        let weapon = TestWeapon {
            rate: Rate::Period(Box::new([Ticks::new(5)])),
            damage: WeaponDamage::Amount(Box::new([Num::int(amount)])),
            kind: DamageKind::new(kind),
            prefers,
            ..TestWeapon::new(enemies, ActionRange::meters(Num::int(5)), Ticks::ZERO)
        };
        Combat::add_weapon(&mut game, weapon)
    };
    let slots = [
        weapon(10, 0, Some(cannon_prefers)),
        weapon(30, 1, Some(missile_prefers)),
        weapon(10, 0, None),
    ]
    .map(|weapon| (weapon, SlotKind::new(0), Rank::new(1)));
    let wielder = wielder(&mut game, ActionSlots::new(slots), &[]);
    // Targets of no armor, and of armors that take a quarter of the second kind, none of the
    // first, and none of either.
    let kinds: Names<DamageKind> = Names::unique(["kinetic", "blast"]);
    let armored = |game: &mut Match, shares: &[(&str, i64, i64)]| {
        let unit_type = Units::load_next_type(game, &UnitTypeCore::default());
        dummy().give(game, unit_type);
        let shares = shares.iter().map(|&(kind, over, under)| {
            let share = Num::int(over) / Num::int(under);
            (
                DeclaredName::new(kind).unwrap(),
                ArmorShare::new(share).unwrap(),
            )
        });
        let armor = Armor::of(&shares.collect(), &kinds).unwrap();
        ById::<UnitType, TypeArmor>::amend(game, |_, book| {
            let armors = Box::new([]);
            book.set(
                unit_type,
                TypeArmor {
                    armor: Some(armor),
                    armors,
                },
            );
        });
        unit_type
    };
    let plain = Units::load_next_type(&mut game, &UnitTypeCore::default());
    for unit_type in [infantry, vehicle, plain] {
        dummy().give(&mut game, unit_type);
    }
    let plated = armored(&mut game, &[("blast", 1, 4)]);
    let shielded = armored(&mut game, &[("kinetic", 0, 1)]);
    let warded = armored(&mut game, &[("kinetic", 0, 1), ("blast", 0, 1)]);
    let pairs = [infantry, vehicle, plain, plated, shielded, warded]
        .into_iter()
        .zip(0..)
        .map(|(unit_type, at)| {
            let attacker = Spawn::of(wielder, (at * 100, 0, 0)).place(&mut game);
            let target = Spawn::of(unit_type, (at * 100 + 1, 0, 0))
                .team(Team::new(1))
                .place(&mut game);
            Combat::attack(&mut game, attacker, target);
            [attacker, target]
        })
        .collect::<Vec<_>>();
    let chosen = |game: &Match| {
        let chosen = pairs
            .iter()
            .map(|&[attacker, _]| game.try_part::<ActionProgress>(attacker).unwrap().weapon());
        chosen.collect::<Vec<_>>()
    };
    let healths = |game: &Match| {
        let healths = pairs.iter().map(|&[_, target]| Stats::health(game, target));
        healths.collect::<Vec<_>>()
    };

    // In tick 0, all ready: infantry takes the cannon it prefers over the stronger missile, and
    // a vehicle the missile; a plain unit the missile, of the most damage. The plated one takes
    // the missile's 30 as 7.5, below the cannon's and the rifle's 10, of which the lower slot,
    // the cannon, comes first; the shielded one spares the cannon and the rifle, and the warded
    // one every weapon.
    game.run_until(1);
    assert_eq!(
        chosen(&game),
        [Some(0), Some(1), Some(1), Some(0), Some(1), None]
    );
    assert_eq!(healths(&game), [90, 70, 70, 90, 70, 100]);
    // In tick 1, each weapon that fired cools until tick 5: infantry keeps the cannon it
    // prefers, which ranks as ready, and waits for it; a plain unit's missile cools, and the
    // cannon, ready, strikes; the plated one's rifle, ready, strikes for 10 over the missile's
    // 7.5; the shielded one has only the missile, and waits for it.
    game.run_until(2);
    assert_eq!(
        chosen(&game),
        [Some(0), Some(1), Some(0), Some(2), Some(1), None]
    );
    assert_eq!(healths(&game), [90, 70, 60, 80, 70, 100]);
}

#[test]
fn a_ground_weapon_selects_a_flier_on_its_pad_and_an_air_weapon_once_it_flies() {
    // A unit of a ground weapon, of the enemies no air target, at place 1, 10, and an air weapon,
    // of the air targets, at place 2, 25, each of 5 m and a windup of a tick, at a period of 5
    // ticks; a flier 1 m off, of a flight at 8 m that climbs a meter a tick from the ground and
    // is an air target past 3 m (04-capabilities/stats.md, Airborne is a tag of height).
    let mut game = Match::builder(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
    ])
    .build();
    Combat::load_weapon_stats(&mut game);
    let rules = ["attack_rate", "damage", "damage_air"]
        .map(|name| (Stat::named(name).unwrap(), StatRule::default()));
    Stats::load_rules(&mut game, &BTreeMap::from(rules));
    let weapon = |game: &mut Match, aim: &str, damage| {
        let aim = game
            .resource::<MatchNames>()
            .resolve_filter(&FilterData::parse(aim).unwrap())
            .unwrap();
        let weapon = TestWeapon {
            damage,
            ..TestWeapon::new(aim, ActionRange::meters(Num::int(5)), Ticks::new(1))
        };
        Combat::add_weapon(game, weapon)
    };
    let ground = weapon(
        &mut game,
        "enemies:!air_target",
        WeaponDamage::Stat(StatId::new(1)),
    );
    let air = weapon(
        &mut game,
        "enemies:air_target",
        WeaponDamage::Stat(StatId::new(2)),
    );
    let kind = SlotKind::new(0);
    let slots = ActionSlots::new([(ground, kind, Rank::new(1)), (air, kind, Rank::new(1))]);
    let shooter = wielder(&mut game, slots, &[Num::int(6), Num::int(10), Num::int(25)]);
    let flier = Units::load_next_type(&mut game, &UnitTypeCore::default());
    dummy().give(&mut game, flier);
    let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
    let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
    Navigation::load_flight(&mut game, flier, flight);
    let id = Spawn::of(shooter, (0, 0, 0)).place(&mut game);
    let target = Spawn::of(flier, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let underway = |game: &Match| {
        let progress = game.try_part::<ActionProgress>(id).unwrap();
        progress.in_progress().map(InProgress::slot)
    };
    // On its pad, only the ground weapon selects it: it starts in tick 0 and strikes in tick 1,
    // for 10.
    Combat::attack(&mut game, id, target);
    game.run_until(1);
    assert_eq!(underway(&game), Some(0));
    game.run_until(2);
    assert_eq!(Stats::health(&game, target), 90);
    // It climbs a meter in each tick from tick 0, 4 m up after tick 3: while its ground weapon
    // cools down, the attack's next start, in tick 4, finds it an air target, and the air
    // weapon alone selects it; it strikes in tick 5 for 25.
    game.run_until(4);
    assert_eq!(underway(&game), None);
    game.run_until(5);
    assert_eq!(underway(&game), Some(1));
    game.run_until(6);
    assert_eq!(Stats::health(&game, target), 65);
}

#[test]
fn a_windup_its_attackers_states_stop_starts_again_and_spends_nothing() {
    let mut game = fight_match();
    let [fighter, dummy] = load_types(&mut game, [fighter(), dummy()]);
    let early = Spawn::of(fighter, (0, 0, 0)).place(&mut game);
    let late = Spawn::of(fighter, (0, 0, 1)).place(&mut game);
    let dummy = Spawn::of(dummy, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, early, dummy);
    Combat::attack(&mut game, late, dummy);
    // Both start in tick 0, to strike in tick 2. The early one is disarmed before tick 1: Act
    // interrupts its windup and keeps its target. The late one is stunned in tick 2's Move
    // stage, after Act, so its strike in Hit is interrupted instead.
    let stunned = &[Block::Move, Block::Attack, Block::Cast, Block::Use];
    Units::block_at(&mut game, late, 2, SimSystems::Move, stunned);
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
            ActionRange::meters(Num::int(2)),
            Ticks::new(2),
        )
    };
    let weapon = Combat::add_weapon(&mut game, weapon);
    let slots = ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]);
    let paying = wielder(&mut game, slots, &[Num::int(6), Num::int(10)]);
    let dummy = dummy().load(&mut game);
    let pools = Pools::new([(mana, Num::int(100))]).unwrap();
    Units::give_type(&mut game, paying, pools);
    let owner = Owner::new(PlayerSlot::new(0));
    let id = Spawn::of(paying, (0, 0, 0)).parts(owner).place(&mut game);
    let dummy = Spawn::of(dummy, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
            ActionRange::meters(Num::int(2)),
            Ticks::new(2),
        )
    };
    let weapon = Combat::add_weapon(&mut game, weapon);
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
    let id = Spawn::of(paying, (0, 0, 0)).place(&mut game);
    let dummy = Spawn::of(dummy, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
    let [slow, quick, prey] = load_types(&mut game, armed);
    let slow = Spawn::of(slow, (0, 0, 0)).place(&mut game);
    let quick = Spawn::of(quick, (0, 0, 1)).place(&mut game);
    let prey = Spawn::of(prey, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, slow, prey);
    Combat::attack(&mut game, quick, prey);

    // Both start in tick 0; the quick one strikes in tick 1 and kills the prey, before the slow
    // one's strike in tick 2. The slow one drops its target in tick 2 and is ready at once.
    game.run_until(2);
    assert!(game.try_part::<Dead>(prey).is_some());
    assert_eq!(Attack::of(&game, slow).started(), Some(Tick::new(0)));
    game.run_until(3);
    assert_eq!(Attack::of(&game, slow), Attack::default());
    assert_eq!(Attack::of(&game, quick).ready_at(), Tick::new(5));
}

#[test]
fn strikes_in_one_tick_see_the_state_before_any_of_them() {
    let mut game = fight_match();
    let duelist = combatant(30, 2, 1, 5, 30).on_death(OnDeath::Stay);
    let [duelist, fighter] = load_types(&mut game, [duelist, fighter()]);
    let first = Spawn::of(duelist, (0, 0, 0)).place(&mut game);
    let second = Spawn::of(duelist, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
    let third = Spawn::of(fighter, (0, 0, 1))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, third, first);
    game.run_until(3);
    assert_eq!(Attack::of(&game, third).target(), None);
}

#[test]
fn targets_are_living_enemies() {
    let mut game = fight_match();
    let prey = combatant(10, 0, 0, 1, 0);
    let [prey, stays] = load_types(&mut game, [prey.clone(), prey.on_death(OnDeath::Stay)]);
    // Up at y = 9: a target all the same.
    let high = Spawn::of(prey, (0, 9, 3)).place(&mut game);
    let far = Spawn::of(prey, (6, 0, 0)).place(&mut game);
    let dead = Spawn::of(stays, (0, 0, 1)).parts(Dead).place(&mut game);
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
        let id = Spawn::of(prey, (x, 0, 5)).place(&mut game);
        Units::set_blocks(&mut game, id, cases[usize::try_from(x).unwrap()].0);
        id
    });

    let enemy_at = |game: &mut Match, team: Team, target: StableId| {
        game.run_system_once(move |targets: Targets<'_, '_>| {
            targets.enemy(team, target).map(|unit| unit.position)
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
        let position = target.then_some(Vec3::from((x, 0, 5)));
        assert_eq!(
            enemy_at(&mut game, Team::new(1), unit),
            position,
            "{blocks:?}"
        );
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
    let spent = Spawn::of(stays, (0, 0, 7)).place(&mut game);
    let full = *game.part::<Life>(spent);
    let read = |game: &mut Match| {
        let entity = game.unit_entity(spent);
        let reached = game
            .run_system_once(move |targets: Targets<'_, '_>| targets.living_at(entity).is_some())
            .unwrap();
        let alive = Units::read(game, "of.is_alive", spent).unwrap();
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
    let [fighter, doomed] = load_types(&mut game, armed.clone());
    let fighter = Spawn::of(fighter, (0, 0, 0)).place(&mut game);
    let doomed = Spawn::of(doomed, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Combat::attack(&mut game, fighter, doomed);
    game.run_until(3);
    assert!(game.try_part::<Dead>(doomed).is_some());

    // A restore loads the match's books first, as the packages give them: the same types and
    // weapons, in the same order.
    let mut restored = fight_match();
    load_types(&mut restored, armed);
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
        let loose = &mut LooseDues::default();
        progress.set_attack_target(Some(AttackTarget::ordered(doomed)), &mut loose.dues());
        progress.schedule_attack(0, Tick::new(resolves_at), &mut loose.dues());
        let mut times = restored.part::<SlotTimes>(fighter).clone();
        times.cool_down(0, Tick::new(ready_at));
        let checked = slots.check(&restored, entity) && progress.check_against(&slots, &restored);
        Binary::decodes_again(&progress) && checked && Binary::decodes_again(&times)
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
        let loose = &mut LooseDues::default();
        progress.set_attack_target(Some(AttackTarget::ordered(doomed)), &mut loose.dues());
        progress.schedule_attack(0, Tick::new(2), &mut loose.dues());
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
    let [fighter] = load_types(&mut game, [fighter()]);
    let id = Spawn::of(fighter, (0, 0, 0)).place(&mut game);
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
            CombatBook::amend(&mut game, |_, book| {
                book.set_assist_window(Some(Ticks::new(ticks)));
            });
        }
        let team = Team::new(0);
        let victim = combatant(130, 0, 0, 1, 0);
        let armed = [
            fighter(),
            slow.clone(),
            victim.clone().on_death(OnDeath::Stay),
            victim,
        ];
        let [fighter, slow, hero, creep] = load_types(&mut game, armed);
        let [first, killer] =
            [4, 6].map(|x| Spawn::of(fighter, (x, 0, 0)).team(team).place(&mut game));
        let slow_one = Spawn::of(slow, (5, 0, 0)).team(team).place(&mut game);
        let owner = Owner::new(PlayerSlot::new(3));
        let hero = Spawn::of(hero, (5, 0, 1))
            .team(Team::new(1))
            .parts(owner)
            .place(&mut game);
        let creep = Spawn::of(creep, (5, 0, -1))
            .team(Team::new(1))
            .place(&mut game);
        let [creep_first, creep_killer] =
            [4, 6].map(|x| Spawn::of(fighter, (x, 0, -1)).team(team).place(&mut game));
        let creep_slow = Spawn::of(slow, (5, 0, -1)).team(team).place(&mut game);
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
            tags: TagSet::NONE,
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
        // The hero stays, dead; the creep too, until its decay a frame's ticks on.
        assert!(game.try_part::<Dead>(hero).is_some());
        assert!(game.try_part::<Decay>(creep).is_some());
    }

    // The hero comes back at the start of its respawn tick, 10, at its spawn point with full
    // health and no attacker on record; the attackers dropped it when it died. Its death and its
    // respawn each tell what aims at it, once.
    let mut game = fight_match();
    game.insert_resource(Heard::default());
    game.add_observer(|died: On<'_, '_, Died>, mut heard: ResMut<'_, Heard>| {
        heard.0.push(("died", died.id));
    });
    game.add_observer(
        |teleported: On<'_, '_, Teleported>, mut heard: ResMut<'_, Heard>| {
            heard.0.push(("teleported", teleported.id));
        },
    );
    let stays = combatant(30, 0, 0, 1, 0).on_death(OnDeath::Stay);
    let [fighter, stays] = load_types(&mut game, [fighter(), stays]);
    let attacker = Spawn::of(fighter, (4, 0, 0)).place(&mut game);
    let spawn = SpawnPoint::new(Vec3::from((-3, 0, 2)));
    let hero = Spawn::of(stays, (5, 0, 0))
        .team(Team::new(1))
        .parts(spawn)
        .place(&mut game);
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
        game.try_part::<Position>(hero)
            .map(|position| position.get()),
        Some(Vec3::from((-3, 0, 2)))
    );
    assert_eq!(Stats::health(&game, hero), 30);
    assert_eq!(attackers(&game), Some(0));
    assert_eq!(Attack::of(&game, attacker).target(), None);
    assert_eq!(
        game.resource::<Heard>().0,
        [("died", hero), ("teleported", hero)]
    );
}

#[test]
fn a_kept_dead_unit_stays_its_time_of_no_target_or_order_and_then_despawns() {
    // A victim of 30 life, killed by one strike, which dies in the tick that just ran.
    let killed = |on_death| {
        let mut game = fight_match();
        let victim = combatant(30, 0, 0, 1, 0).on_death(on_death);
        let [fighter, victim] = load_types(&mut game, [fighter(), victim]);
        let attacker = Spawn::of(fighter, (4, 0, 0)).place(&mut game);
        let victim = Spawn::of(victim, (5, 0, 0))
            .team(Team::new(1))
            .place(&mut game);
        Combat::attack(&mut game, attacker, victim);
        while game.try_part::<Dead>(victim).is_none() {
            game.step();
        }
        let died = game.now().get() - 1;
        let decay = game
            .try_part::<Decay>(victim)
            .map(|decay| decay.at.get() - died);
        (game, attacker, victim, died, decay)
    };
    // Kept 500 ms, 15 ticks at 30 a second, ⌈500 · 30 / 1000⌉, it decays at the end of the 15th
    // tick after; kept 100 ms, 3 ticks, or despawned at its death, at the end of the 9th, the
    // most a server frame runs, so a frame's send holds its death; one that stays, never.
    let keep = |ms| OnDeath::Keep {
        ms: NonZeroU32::new(ms).unwrap(),
    };
    let decays =
        [keep(500), keep(100), OnDeath::Despawn, OnDeath::Stay].map(|on_death| killed(on_death).4);
    assert_eq!(decays, [Some(15), Some(9), Some(9), None]);
    assert_eq!(SimTick::FRAME_MOST, Ticks::new(9));
    let (mut game, attacker, victim, died, _) = killed(keep(500));
    // Its attacker drops it in the next tick's attack step, and an order to attack it again
    // takes no target; it stays through the tick before its decay's.
    game.step();
    assert_eq!(Attack::of(&game, attacker).target(), None);
    Combat::attack(&mut game, attacker, victim);
    game.run_until(died + 15);
    assert!(game.try_part::<Dead>(victim).is_some());
    assert_eq!(Attack::of(&game, attacker).target(), None);
    // The tick of its decay ends with it gone.
    game.step();
    assert!(game.try_part::<Dead>(victim).is_none());
    assert!(game.try_part::<Position>(victim).is_none());
}

/// The units each event of a death or a teleport named, in the order heard.
#[derive(Resource, Debug, Default)]
struct Heard(Vec<(&'static str, StableId)>);

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
    let pools = Names::<PoolId>::unique([health.as_str()]);
    let life = combat.life_pool(&pools).unwrap();
    let bound = CombatBook::new(&combat, Match::TEST_RATE, |stat| {
        book.named(stat).ok_or_else(|| stat.clone())
    });
    world.insert_resource(bound.unwrap());
    Stats::load_book(world, book);
    PoolBook::amend(world, |_, book| book.set_life(life));
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
    allegiance: Allegiance {
        team: Team::new(0),
        owner: None,
    },
};

#[test]
fn the_pass_deals_damage_in_its_order_and_credits_the_kill() {
    let mut game = fight_match();
    CombatBook::amend(&mut game, |_, book| {
        book.set_assist_window(Some(Ticks::new(10)));
    });
    let [dummy, stays] = load_types(&mut game, [dummy(), dummy().on_death(OnDeath::Stay)]);
    let a = Spawn::of(dummy, (0, 0, 0)).place(&mut game);
    let b = Spawn::of(dummy, (1, 0, 0)).place(&mut game);
    let target = Spawn::of(stays, (2, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
    let c = Spawn::of(dummy, (3, 0, 0)).place(&mut game);
    Combat::damage(&mut game, Some(c), target, 10, ATTACK);
    game.run_until(2);
    Combat::heal_unit(&mut game, target, 50);
    assert_eq!(Stats::health(&game, target), 0);
    let attackers = game.try_part::<RecentAttackers>(target).unwrap();
    assert!(attackers.iter().all(|attack| attack.source != c));
    assert_eq!(game.resource::<Deaths>().iter().count(), 0);

    // An invulnerable unit takes nothing and records no attacker; untargetable, it takes all.
    let guarded = Spawn::of(dummy, (4, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let hidden = Spawn::of(dummy, (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
    let first = Spawn::of(dummy, (6, 0, 0)).place(&mut game);
    let second = Spawn::of(dummy, (7, 0, 0)).place(&mut game);
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
    let [fighter, stays] = load_types(&mut game, armed.clone());
    let attacker = Spawn::of(fighter, (0, 0, 0)).place(&mut game);
    let target = Spawn::of(stays, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
        .in_set(SimEdgeSystems::After(SimSystems::Mode))
        .after(StatsSystems::Refresh(SimEdgeSystems::After(
            SimSystems::Mode,
        )));
    game.add_tick_systems(hook);
    game.run_until(2);
    assert_eq!(Stats::health(&game, target), 100);
    assert!(game.try_part::<Dead>(attacker).is_none());

    // In tick 2 the pass deals the 30 first, to 70; the attacker, at 0, strikes nothing, where
    // its 30 would leave 40, and dies with no killer. A match restored at the boundary between
    // the ticks holds the damage still to come, and plays tick 2 alike.
    let mut restored = fight_match();
    load_types(&mut restored, armed);
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
    let source = Spawn::of(source, (0, 0, 0)).place(&mut game);
    let target = Spawn::of(dummy, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let scaled = Spawn::of(scaled, (0, 0, 2)).place(&mut game);
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
        assert_eq!(clocks.is_shielded(), held > 0);
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
    let foe = Spawn::of(dummy, (2, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    assert!(source < foe);
    game.part_mut::<Life>(source).take(Num::int(90));
    Combat::damage(&mut game, Some(foe), source, 12, DamageCause::Effect);
    Combat::damage(&mut game, Some(source), target, 20, ATTACK);
    game.run_until(3);
    assert_eq!(game.part::<Life>(source).current(), Num::int(3));
    assert_eq!(game.part::<Life>(target).current(), Num::int(35));
    // Without the bindings the same stats do nothing: in a match whose combat binds its life
    // alone, a source at 50 heals nothing from an attack of 10, which takes the target from 100
    // to 90, and a heal of 10 is whole.
    let mut game = fight_match();
    load_damage_stats(&mut game);
    Combat::bind_life(&mut game, PoolId::FIRST);
    let [dummy, source] = [self::dummy(), self::dummy()].map(|armed| armed.load(&mut game));
    Stats::give_type_stats(&mut game, source, &[-half, half, Num::ONE / 4]);
    let pools = Pools::new([(mana, Num::int(100))]).unwrap();
    Units::give_type(&mut game, source, pools);
    let source = Spawn::of(source, (0, 0, 0)).place(&mut game);
    let target = Spawn::of(dummy, (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let entity = game.unit_entity(source);
    game.get_mut::<Life>(entity).unwrap().take(Num::int(50));
    Combat::damage(&mut game, Some(source), target, 10, ATTACK);
    game.run_until(1);
    assert_eq!(game.part::<Life>(target).current(), Num::int(90));
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
    let [dummy, attacker] = load_types(&mut game, [dummy(), combatant(100, 2, 0, 5, 30)]);
    let dummy = Spawn::of(dummy, (0, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let attackers: Vec<_> = (0..16)
        .map(|_| Spawn::of(attacker, (1, 0, 0)).place(&mut game))
        .collect();
    // Each attacker's windup ends in tick 0; the roll of each, by the attackers' order.
    let rolls = |game: &mut Match| {
        for &attacker in &attackers {
            ProgressDues::change_unit(game, attacker, |progress, dues| {
                progress.set_attack_target(Some(AttackTarget::ordered(dummy)), dues);
                progress.schedule_attack(0, Tick::new(0), dues);
            });
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
        let [fighter, dummy] = load_types(game, [fighter(), dummy()]);
        let own = Spawn::of(fighter, (0, 0, 0)).place(game);
        Spawn::of(dummy, (0, 0, 1)).place(game);
        let enemy = Spawn::of(fighter, (1, 0, 0))
            .team(Team::new(1))
            .parts(Unpredicted)
            .place(game);
        Spawn::of(dummy, (9, 0, 0))
            .team(Team::new(1))
            .parts(Unpredicted)
            .place(game);
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
        let [fighter] = load_types(game, [fighter()]);
        let own = Spawn::of(fighter, (0, 0, 0)).place(game);
        let enemy = Spawn::of(fighter, (5, 0, 0))
            .team(Team::new(1))
            .parts(Unpredicted)
            .place(game);
        let peek = move |world: &mut World| {
            let enemy = world.entity_of(enemy).unwrap();
            if world.get::<RecentAttackers>(enemy).is_some() {
                let own = world.entity_of(own).unwrap();
                *world.get_mut::<Position>(own).unwrap() = Position::from((1, 0, 0));
            }
        };
        let peek = peek.in_set(SimEdgeSystems::After(SimSystems::Mode));
        let after = peek.after(StatsSystems::Refresh(SimEdgeSystems::After(
            SimSystems::Mode,
        )));
        game.add_tick_systems(after);
    });
}

mod slow_weapons;
mod streaks;
mod turrets;
mod wings;
