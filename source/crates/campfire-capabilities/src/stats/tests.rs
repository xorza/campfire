use std::collections::BTreeMap;
use std::slice;

use bevy_ecs::entity::Entity;
use campfire_common::{Tick, Toml};
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, IdAllocator, Position, SimComponent, SimUpdate};

use super::*;
use crate::capability_set::test_match::TestMatch;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::application::{Application, NewInstance};
use crate::stats::held_modifiers::Held;
use crate::stats::instance::StatShare;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_clocks::Clock;
use crate::stats::modifier_data::{AuraData, ModifierData, Reapply};
use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_change::StatChange;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_data::{StatValue, StatsData};
use crate::stats::unit_stats::UnitStats;
use crate::units::Units;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::move_step::MoveStep;
use crate::units::tag_book::TagBook;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::{Number, ParamRef};
use crate::values::param::{Param, Scaling};
use crate::values::rank::Rank;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::{EngineStat, Stat};

/// `value` sixteenths.
fn sixteenths(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 4))
}

/// The rules of the test: move speed at most 5, `armor` at least −30, a weapon's stats, and the
/// pools' stats.
fn rules() -> BTreeMap<Stat, StatRule> {
    let free = StatRule::default();
    let speed = StatRule {
        min: None,
        max: Some(Num::int(5)),
    };
    let armor = StatRule {
        min: Some(Num::int(-30)),
        max: None,
    };
    let named = [
        "health",
        "health_regen",
        "mana",
        "attack_speed",
        "attack_damage",
    ];
    [
        (Stat::Engine(EngineStat::MoveSpeed), speed),
        (armor_stat(), armor),
    ]
    .into_iter()
    .chain(named.map(|name| (stat(name), free)))
    .collect()
}

fn stat(name: &str) -> Stat {
    Stat::named(name).unwrap()
}

fn armor_stat() -> Stat {
    stat("armor")
}

/// The pools of the test, by pool id: `health`, which regenerates, then `mana`, which does not.
const HEALTH: PoolId = PoolId::FIRST;
const MANA: PoolId = PoolId::new(1).unwrap();

fn pools() -> BTreeMap<DeclaredName, PoolData> {
    let pool = |max: &str, regen: Option<&str>| PoolData {
        max: stat(max),
        regen: regen.map(stat),
    };
    BTreeMap::from([
        (
            DeclaredName::new("health").unwrap(),
            pool("health", Some("health_regen")),
        ),
        (DeclaredName::new("mana").unwrap(), pool("mana", None)),
    ])
}

/// A unit type's stats, each a base and a gain a level.
fn stats(values: &[(Stat, Num, Num)]) -> StatsData {
    StatsData(
        values
            .iter()
            .map(|(stat, base, per_level)| {
                let value = StatValue {
                    base: *base,
                    per_level: *per_level,
                };
                (stat.clone(), value)
            })
            .collect(),
    )
}

/// A match with the stats capability and a book of `types`, with a move speed cap of 6.
fn stat_match(types: &[StatsData]) -> TestMatch {
    let mut game = TestMatch::client(&[Capability::Stats]);
    let types = types.iter().enumerate().map(|(at, data)| {
        let unit_type = UnitType::new(u16::try_from(at).unwrap());
        (unit_type, data)
    });
    let book = StatBook::new(&rules(), types, Num::int(6));
    let pools = PoolBook::new(&pools(), &book);
    Stats::load(&mut game.world, book, pools);
    game
}

/// A unit of `unit_type` at level 1 that walks and has a pool of health and of mana, its values
/// before the stats derive them.
fn unit(game: &mut TestMatch, unit_type: u16) -> Entity {
    let id = game.world.resource_mut::<IdAllocator>().allocate();
    game.world
        .spawn((
            id,
            UnitType::new(unit_type),
            Level::default(),
            UnitStats::default(),
            UnitTags::default(),
            MoveStep::new(Num::ZERO).unwrap(),
            Pools::new([(HEALTH, Num::ONE), (MANA, Num::ONE)]).unwrap(),
        ))
        .id()
}

/// A modifier of no stats, tags, params or times that gives `aura`.
fn modifier_data(aura: Option<AuraData>) -> ModifierData {
    ModifierData {
        aura,
        ..ModifierData::default()
    }
}

#[test]
fn a_units_stats_follow_its_type_and_level_within_their_limits() {
    // A hero of health 380 + 76 a level, regen 1.5, mana 250 + 45, attack damage 51 + 3,
    // attack speed 0.625 + 0.0625 and move speed 4 + 0.25.
    let hero = stats(&[
        (stat("health"), Num::int(380), Num::int(76)),
        (stat("health_regen"), sixteenths(24), Num::ZERO),
        (stat("mana"), Num::int(250), Num::int(45)),
        (stat("attack_damage"), Num::int(51), Num::int(3)),
        (stat("attack_speed"), sixteenths(10), sixteenths(1)),
        (
            Stat::Engine(EngineStat::MoveSpeed),
            Num::int(4),
            sixteenths(4),
        ),
    ]);
    let mut game = stat_match(&[hero]);
    let units = [unit(&mut game, 0)];
    game.step();
    let get = |game: &TestMatch| {
        let world = &game.world;
        let unit = world.entity(units[0]);
        let book = world.resource::<StatBook>();
        let values = unit.get::<UnitStats>().unwrap().values();
        let value = |name| values[book.named(&stat(name)).unwrap().index()];
        (
            unit.get::<MoveStep>().unwrap().get(),
            value("attack_speed"),
            value("attack_damage"),
            *unit.get::<Pools>().unwrap(),
        )
    };
    let amounts = |pools: Pools, pool| (pools.current(pool).unwrap(), pools.max(pool).unwrap());
    // At level 1: 4 m/s, 4 × 2²⁴ ÷ 30 = 2 236 962.13 bits a tick, to 2 236 962; 0.625 attacks a
    // second; damage 51; full health of 380 and mana of 250.
    let (step, rate, damage, pools) = get(&game);
    assert_eq!(step, Num::from_bits(2_236_962));
    assert_eq!((rate, damage), (sixteenths(10), Num::int(51)));
    assert_eq!(amounts(pools, HEALTH), (Num::int(380), Num::int(380)));
    assert_eq!(amounts(pools, MANA), (Num::int(250), Num::int(250)));

    // Down 80 to 300 of 380, then at level 18: health 380 + 76 × 17 = 1672, the pool up by the
    // 1292 the maximum rose, to 1592, plus the regen of the tick that runs, 1.5 ÷ 30 = 0.05,
    // 838 860.8 bits, to 838 860 and its fifth carried; attack speed 0.625 + 17 × 0.0625 =
    // 1.6875, 27 sixteenths; damage 51 + 51 = 102; move speed 4 + 4.25 = 8.25, past the limit of
    // 5, 5 × 2²⁴ ÷ 30 = 2 796 202.67, to 2 796 203. Mana, full and with no regen, rises with its
    // maximum, to 250 + 45 × 17 = 1015.
    let mut pools = game.world.get_mut::<Pools>(units[0]).unwrap();
    pools.take(HEALTH, Num::int(80));
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::new(18).unwrap();
    game.step();
    let (step, rate, damage, pools) = get(&game);
    assert_eq!(step, Num::from_bits(2_796_203));
    assert_eq!((rate, damage), (sixteenths(27), Num::int(102)));
    let regen = Num::from_bits(838_860);
    assert_eq!(
        amounts(pools, HEALTH),
        (Num::int(1592) + regen, Num::int(1672))
    );
    assert_eq!(amounts(pools, MANA), (Num::int(1015), Num::int(1015)));

    // Over the next 29 ticks the regen adds the rest of the second: 30 ticks gain exactly 1.5.
    for _ in 0..29 {
        game.step();
    }
    let pools = game.world.get::<Pools>(units[0]).unwrap();
    assert_eq!(pools.current(HEALTH), Some(Num::int(1592) + sixteenths(24)));

    // Back to level 1: the maximum falls to 380, and the current amount to it.
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::default();
    game.step();
    let (_, _, _, pools) = get(&game);
    assert_eq!(amounts(pools, HEALTH), (Num::int(380), Num::int(380)));
}

/// An application from `source` of modifier `id`, changing `stat` by `value` a stack with `op`,
/// refreshed or stacked as `reapply` says.
fn share(id: u16, source: Option<StableId>, value: Num, reapply: Reapply) -> Application {
    Application {
        instance: NewInstance {
            stats: vec![StatShare { value, live: None }],
            ..NewInstance::bare(ModifierId::new(id), source)
        },
        reapply,
        max_stacks: None,
    }
}

#[test]
fn a_stat_is_its_base_plus_adds_times_pcts_times_the_largest_cut() {
    // Armor 10 + 2 a level, at level 3: 10 + 2 × 2 = 14.
    let armored = StatsData(
        [(
            armor_stat(),
            StatValue {
                base: Num::from_int(10).unwrap(),
                per_level: Num::from_int(2).unwrap(),
            },
        )]
        .into(),
    );
    let tenths = |tenths: i64| Num::int(tenths) / 10;
    let value = |changes: &[(StatOp, Num, u32)]| {
        let mut game = stat_match(slice::from_ref(&armored));
        let armor = game
            .world
            .resource::<StatBook>()
            .named(&armor_stat())
            .unwrap();
        let mut applications = Vec::new();
        for (at, &(op, value, stacks)) in changes.iter().enumerate() {
            ModifierBook::push_changes(&mut game.world, &[(armor, op)], TagSet::default());
            let id = u16::try_from(at).unwrap();
            for _ in 0..stacks {
                applications.push(share(id, None, value, Reapply::Stack));
            }
        }
        let armored_unit = unit(&mut game, 0);
        let mut entity = game.world.entity_mut(armored_unit);
        entity.insert((Level::new(3).unwrap(), Modifiers::bundle(applications)));
        game.step();
        let stats = game.world.get::<UnitStats>(armored_unit).unwrap();
        stats.values()[armor.index()]
    };
    assert_eq!(value(&[]), Num::int(14));
    // Adds of 6 and of −1 three times: 17. Pcts of 0.3, 5 033 164.8 bits to 5 033 165, and of
    // −0.1, 1 677 721.6 to 1 677 722: 2²⁴ + 3 355 443 = 20 132 659 bits. Cuts of 0.1 and 0.25,
    // only the larger counting: 0.75. 17 × 20 132 659 × 0.75 = 256 691 402.25 bits, rounded once
    // to 256 691 402.
    let mixed = [
        (StatOp::Add, Num::int(6), 1),
        (StatOp::Add, Num::int(-1), 3),
        (StatOp::Pct, tenths(3), 1),
        (StatOp::Pct, tenths(-1), 1),
        (StatOp::Cut, tenths(1), 1),
        (StatOp::Cut, sixteenths(4), 1),
    ];
    assert_eq!(value(&mixed), Num::from_bits(256_691_402));
    // A cut past 1 counts as 1, and the value is 0.
    let cut = [mixed.as_slice(), &[(StatOp::Cut, sixteenths(24), 1)]].concat();
    assert_eq!(value(&cut), Num::ZERO);
    // A negative value keeps its sign: (14 − 20) × 1.5 × 0.75 = −6.75. Past the limit, (14 − 40)
    // × 1.5 = −39 stops at −30.
    let negative = [
        (StatOp::Add, Num::int(-20), 1),
        (StatOp::Pct, sixteenths(8), 1),
        (StatOp::Cut, sixteenths(4), 1),
    ];
    assert_eq!(value(&negative), -Num::int(6) - sixteenths(12));
    let floored = [
        (StatOp::Add, Num::int(-40), 1),
        (StatOp::Pct, sixteenths(8), 1),
    ];
    assert_eq!(value(&floored), Num::int(-30));
}

#[test]
fn modifiers_change_a_units_stats_and_tags_hold_them_without_effect() {
    // Move speed 4. The tags: 0 detects, and is the type's own; 1, `slowed`, has no effect; 2,
    // `slow_immune`, makes immune to 1.
    let walker = stats(&[(Stat::Engine(EngineStat::MoveSpeed), Num::int(4), Num::ZERO)]);
    let mut game = stat_match(&[walker]);
    let [sight, slowed, slow_immune] = [0, 1, 2].map(Tag::new);
    let effects = [
        (TagProperties::default().with_detects(), TagSet::default()),
        (TagProperties::default(), TagSet::default()),
        (TagProperties::default(), TagSet::of([slowed])),
    ];
    let own = TagSet::of([sight]);
    game.world
        .insert_resource(TagBook::new(effects, [(UnitType::new(0), own)]));
    let unit = unit(&mut game, 0);
    game.world.entity_mut(unit).insert(Modifiers::default());
    let book = game.world.resource::<StatBook>();
    let speed = book.named(&Stat::Engine(EngineStat::MoveSpeed)).unwrap();
    let mut ids = IdAllocator::default();
    let (first, second) = (Some(ids.allocate()), Some(ids.allocate()));
    // The modifiers, by id: a slowing cut, an add, a pct, an untagged cut, an immunity, and a
    // cut that grants both `slowed` and the immunity to it.
    let modifiers = [
        (StatOp::Cut, TagSet::of([slowed])),
        (StatOp::Add, TagSet::default()),
        (StatOp::Pct, TagSet::default()),
        (StatOp::Cut, TagSet::default()),
        (StatOp::Add, TagSet::of([slow_immune])),
        (StatOp::Cut, TagSet::of([slowed, slow_immune])),
    ];
    for (op, tags) in modifiers {
        ModifierBook::push_changes(&mut game.world, &[(speed, op)], tags);
    }
    let slowing = |source, value| share(0, source, value, Reapply::Refresh);
    // Cuts of 0.25 and 0.5 from two sources, each `slowed`: only the larger counts. An add of
    // 0.5 that stacks, twice: +1.
    let mut carried = CarriedMut::of(&mut game.world, unit).unwrap();
    carried.apply(slowing(first, sixteenths(4)));
    carried.apply(slowing(second, sixteenths(8)));
    for _ in 0..2 {
        let add = share(1, first, sixteenths(8), Reapply::Stack);
        carried.apply(add);
    }
    game.step();
    // (4 + 1) × (1 − 0.5) = 2.5 m/s, 2.5 × 2²⁴ ÷ 30 = 1 398 101.33 bits a tick, to 1 398 101.
    let step = |game: &TestMatch| game.world.get::<MoveStep>(unit).unwrap().get();
    assert_eq!(step(&game), Num::from_bits(1_398_101));
    let tags = |game: &TestMatch| *game.world.get::<UnitTags>(unit).unwrap();
    assert_eq!(tags(&game).tags, TagSet::of([sight, slowed]));
    assert!(tags(&game).properties.detects());

    // A pct of 0.25: 5 × 1.25 × 0.5 = 3.125 m/s, 3.125 × 2²⁴ ÷ 30 = 1 747 626.67 bits, to
    // 1 747 627. An untagged cut of 1.5 counts as 1: no step at all.
    let mut carried = CarriedMut::of(&mut game.world, unit).unwrap();
    carried.apply(share(2, first, sixteenths(4), Reapply::Refresh));
    game.step();
    assert_eq!(step(&game), Num::from_bits(1_747_627));
    let mut carried = CarriedMut::of(&mut game.world, unit).unwrap();
    carried.apply(share(3, first, sixteenths(24), Reapply::Refresh));
    game.step();
    assert_eq!(step(&game), Num::ZERO);

    // The cut of 1.5 removed, slow immunity from a modifier of 2 stacks and no stat value holds
    // every `slowed` modifier without effect, so no cut counts: 5 × 1.25 = 6.25, past the limit
    // of 5: 5 × 2²⁴ ÷ 30 = 2 796 202.67 bits, to 2 796 203. `slowed` leaves the unit's tags, and
    // the immunity joins them.
    let mut carried = CarriedMut::of(&mut game.world, unit).unwrap();
    carried.remove(ModifierId::new(3), first);
    let immune = share(4, first, Num::ZERO, Reapply::Stack);
    carried.apply(immune.clone());
    carried.apply(immune);
    game.step();
    assert_eq!(step(&game), Num::from_bits(2_796_203));
    assert_eq!(tags(&game).tags, TagSet::of([sight, slow_immune]));
    assert_eq!(tags(&game).immune, TagSet::of([slowed]));

    // A modifier that grants both an immunity and the tag it is immune to holds itself: its
    // cut of 0.25 counts, whatever order the modifiers are in. 6.25 × 0.75 = 4.6875 m/s, 4.6875
    // × 2²⁴ ÷ 30 = 2 621 440 bits exactly.
    let both = share(5, second, sixteenths(4), Reapply::Refresh);
    CarriedMut::of(&mut game.world, unit).unwrap().apply(both);
    game.step();
    assert_eq!(step(&game), Num::from_bits(2_621_440));
    assert_eq!(tags(&game).tags, TagSet::of([sight, slowed, slow_immune]));

    // The immunity ends: the held cuts act again, the larger 0.5: 3.125 m/s.
    let mut carried = CarriedMut::of(&mut game.world, unit).unwrap();
    carried.remove(ModifierId::new(4), first);
    carried.remove(ModifierId::new(5), second);
    game.step();
    assert_eq!(step(&game), Num::from_bits(1_747_627));
    assert_eq!(tags(&game).immune, TagSet::default());

    // Removing every modifier: back to 4 m/s, and to the type's tag alone.
    *game.world.get_mut::<Modifiers>(unit).unwrap() = Modifiers::default();
    game.step();
    assert_eq!(step(&game), Num::from_bits(2_236_962));
    assert_eq!(tags(&game).tags, own);
}

#[test]
fn a_modifier_that_reads_a_param_applies_in_a_match_with_no_scripts() {
    // Move speed 4, and a modifier whose cut reads its own param of 0.5: it applies, as the
    // stats read the params from the param book. 4 × 0.5 = 2 m/s, 2 × 2²⁴ ÷ 30 = 1 118 481.07
    // bits, to 1 118 481.
    let walker = stats(&[(Stat::Engine(EngineStat::MoveSpeed), Num::int(4), Num::ZERO)]);
    let mut game = stat_match(&[walker]);
    let unit = unit(&mut game, 0);
    game.world.entity_mut(unit).insert(Modifiers::default());
    let half = Param::Ranked(Ranked::One(Scalar::Decimal(sixteenths(8))));
    let cut = StatChange {
        op: StatOp::Cut,
        value: Number::Param(ParamRef {
            param: DeclaredName::new("slow").unwrap(),
        }),
    };
    let data = ModifierData {
        stats: BTreeMap::from([(Stat::Engine(EngineStat::MoveSpeed), cut)]),
        params: BTreeMap::from([(DeclaredName::new("slow").unwrap(), half)]),
        ..modifier_data(None)
    };
    Stats::load_modifier(&mut game.world, 0, "slow", &data, None);
    let slow = Stats::modifier(&game.world, 0, "slow").unwrap();
    let target = *game.world.get::<StableId>(unit).unwrap();
    internals::give_modifier(&mut game.world, target, slow, None, false);
    game.step();
    let step = game.world.get::<MoveStep>(unit).unwrap().get();
    assert_eq!(step, Num::from_bits(1_118_481));
}

/// An application of the aura `id` of `radius`, one stack with no end, carried by `carrier`.
fn aura(id: ModifierId, carrier: StableId, radius: Num) -> Application {
    Application {
        instance: NewInstance {
            aura_radius: Some(radius),
            ..NewInstance::bare(id, Some(carrier))
        },
        reapply: Reapply::Refresh,
        max_stacks: None,
    }
}

/// A match of a unit type of armor 10, and the modifier `warding`, whose aura's radius and
/// shield read the own params `reach` and `ward`: 0 and −1 per point of its source's armor, and
/// 0 and −2 per point; and which runs an interval each second.
fn warding_match() -> (TestMatch, ModifierId) {
    let armored = stats(&[(armor_stat(), Num::int(10), Num::ZERO)]);
    let mut game = stat_match(&[armored]);
    Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "armored",
        &UnitTypeData::default(),
    );
    Stats::load_modifier(&mut game.world, 0, "heartened", &modifier_data(None), None);
    let param = |name: &str| {
        Number::Param(ParamRef {
            param: DeclaredName::new(name).unwrap(),
        })
    };
    let against_armor = |per_point: i64| {
        Param::Scaling(Scaling {
            base: Ranked::One(Num::ZERO),
            per_level: Num::ZERO,
            bonus: BTreeMap::new(),
            ratios: BTreeMap::from([(armor_stat(), Num::int(per_point))]),
        })
    };
    let data = ModifierData {
        shield: Some(param("ward")),
        interval_ms: Some(Number::Value(Scalar::Int(1000))),
        params: BTreeMap::from([
            (DeclaredName::new("reach").unwrap(), against_armor(-1)),
            (DeclaredName::new("ward").unwrap(), against_armor(-2)),
        ]),
        ..modifier_data(Some(AuraData {
            radius: param("reach"),
            affects: FilterData::parse("allies").unwrap(),
            modifier: DeclaredName::new("heartened").unwrap(),
        }))
    };
    Stats::load_modifier(&mut game.world, 0, "warding", &data, None);
    let warding = Stats::modifier(&game.world, 0, "warding").unwrap();
    (game, warding)
}

#[test]
fn a_scaling_aura_radius_and_shield_below_zero_hold_zero_and_restore() {
    // The load refuses a negative value and rank, but a scaling param still gives one as it
    // applies: −1 × 10 armor is −10 m of reach, and −2 × 10 is −20 of shield. Each holds 0, as
    // the decode of an instance refuses one below zero, so the snapshot restores.
    let (mut game, warding) = warding_match();
    let unit = unit(&mut game, 0);
    game.world.entity_mut(unit).insert(Modifiers::default());
    game.step();
    let id = *game.world.get::<StableId>(unit).unwrap();
    internals::give_modifier(&mut game.world, id, warding, Some((id, None, 1)), false);
    let modifiers = game.world.get::<Modifiers>(unit).unwrap();
    let carried = modifiers.get(warding, Some(id)).unwrap();
    let clocks = game.world.get::<ModifierClocks>(unit).unwrap();
    let shield = clocks.shield_of(modifiers, warding, Some(id));
    assert_eq!(
        (carried.instance.aura_radius, shield),
        (Some(Num::ZERO), Some(Num::ZERO))
    );
    let (mut fresh, _) = warding_match();
    game.restore_into(&mut fresh);
}

#[test]
fn a_restored_clock_has_the_interval_and_the_shield_its_modifier_has_within_the_limit() {
    // The warding unit's clock, as the application made it, has an interval and a shield, as
    // its modifier does. Without either, the clock would not do what its modifier does; an
    // interval past the limit, whose next tick would overflow, fails to decode.
    let (mut game, warding) = warding_match();
    let unit = unit(&mut game, 0);
    game.world.entity_mut(unit).insert(Modifiers::default());
    game.step();
    let id = *game.world.get::<StableId>(unit).unwrap();
    internals::give_modifier(&mut game.world, id, warding, Some((id, None, 1)), false);
    let made = game.world.get::<ModifierClocks>(unit).unwrap().clone();
    let check = |change: fn(&mut Clock)| {
        let mut clocks = made.clone();
        change(clocks.clock_mut(0));
        TestMatch::decodes(&clocks) && clocks.check(&game.world, unit)
    };
    let limit = |clock: &mut Clock| {
        let interval = clock.interval.as_mut().unwrap();
        (interval.every, interval.next) = (Ticks::LIMIT, Tick::LIMIT);
    };
    assert!(check(|_| ()) && check(limit));
    assert!(!check(|clock| clock.interval = None));
    assert!(!check(|clock| clock.shield = None));
    let past_every = |clock: &mut Clock| {
        clock.interval.as_mut().unwrap().every = Ticks::new(Ticks::LIMIT.get() + 1);
    };
    let past_next = |clock: &mut Clock| {
        clock.interval.as_mut().unwrap().next = Tick::new(Tick::LIMIT.get() + 1);
    };
    assert!(!check(past_every) && !check(past_next));
}

#[test]
fn an_aura_holds_its_modifier_on_the_units_it_selects_within_its_radius() {
    let data = modifier_data;
    let limits = ScriptLimits::ROOMY;
    let scripts = ScriptBudgets::new(limits, 1);
    let mut game = TestMatch::server(&[Capability::Stats], scripts);
    let book = StatBook::new(&rules(), [], Num::int(6));
    Stats::load(&mut game.world, book, PoolBook::default());
    // A presence of 2 m on allies, holding `inspired`.
    let presence = AuraData {
        radius: Number::Value(Scalar::Int(2)),
        affects: FilterData::parse("allies").unwrap(),
        modifier: DeclaredName::new("inspired").unwrap(),
    };
    Stats::load_modifier(&mut game.world, 0, "inspired", &data(None), None);
    Stats::load_modifier(&mut game.world, 0, "presence", &data(Some(presence)), None);
    let at = |x: i64| Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::ZERO)).unwrap();
    let spawn = |game: &mut TestMatch, x: i64, team: u8| {
        game.spawn(at(x), (Team::new(team), Modifiers::default()))
    };
    // The carrier at 0; an ally at 2, on the edge, and one at 3; an enemy at 1.
    let carrier = spawn(&mut game, 0, 0);
    let near = spawn(&mut game, 2, 0);
    let far = spawn(&mut game, 3, 0);
    let enemy = spawn(&mut game, 1, 1);
    let entity =
        |game: &TestMatch, id: StableId| game.world.resource::<EntityIndex>().get(id).unwrap();
    let [inspired, presence] = ["inspired", "presence"].map(|name| {
        game.world
            .resource::<ModifierBook>()
            .named(0, name)
            .unwrap()
    });
    let holder = entity(&game, carrier);
    let mut presence_carrier = CarriedMut::of(&mut game.world, holder).unwrap();
    presence_carrier.apply(aura(presence, carrier, Num::int(2)));
    let holds = |game: &TestMatch, id: StableId| {
        let modifiers = game.world.get::<Modifiers>(entity(game, id)).unwrap();
        modifiers
            .get(inspired, Some(carrier))
            .is_some_and(|instance| {
                instance.lifetime.held_by(Hold::Held) && instance.lifetime.until().is_none()
            })
    };
    game.step();
    // The carrier is its own ally, within 0 m of itself.
    assert_eq!(
        [carrier, near, far, enemy].map(|id| holds(&game, id)),
        [true, true, false, false]
    );
    // The near ally leaves, and the far one comes to 3 m with a body of 1 m: its edge is within
    // 2 m, as an area's radius reaches it.
    *game.world.get_mut::<Position>(entity(&game, near)).unwrap() = at(5);
    let moved = (at(-3), Body::new(Num::int(1)).unwrap());
    game.world.entity_mut(entity(&game, far)).insert(moved);
    game.step();
    assert_eq!([near, far].map(|id| holds(&game, id)), [false, true]);
    // An aura's instance of no stack projects nothing, nor does one its carrier's immunity
    // suppresses: the aura grants tag 0, to which the carrier is immune.
    let stacks = |game: &mut TestMatch, stacks| {
        let holder = entity(game, carrier);
        let mut modifiers = game.world.get_mut::<Modifiers>(holder).unwrap();
        modifiers.set_stacks(presence, Some(carrier), stacks, Tick::new(0));
    };
    stacks(&mut game, 0);
    game.step();
    assert!(!holds(&game, far));
    stacks(&mut game, 1);
    game.step();
    assert!(holds(&game, far));
    let mut book = game.world.resource_mut::<ModifierBook>();
    book.grant_tags(presence, TagSet::of([Tag::new(0)]));
    let immune = UnitTags {
        immune: TagSet::of([Tag::new(0)]),
        ..UnitTags::default()
    };
    game.world.entity_mut(entity(&game, carrier)).insert(immune);
    game.step();
    assert!(!holds(&game, far));
    game.world
        .entity_mut(entity(&game, carrier))
        .remove::<UnitTags>();
    // The carrier dies: its aura goes, with its own modifiers.
    let dead = entity(&game, carrier);
    game.world.entity_mut(dead).insert(Dead);
    game.step();
    assert_eq!([carrier, far].map(|id| holds(&game, id)), [false, false]);
}

#[test]
fn a_modifier_another_capability_holds_lasts_only_its_tick() {
    let limits = ScriptLimits::ROOMY;
    let scripts = ScriptBudgets::new(limits, 1);
    let mut game = TestMatch::server(&[Capability::Stats], scripts);
    let book = StatBook::new(&rules(), [], Num::int(6));
    Stats::load(&mut game.world, book, PoolBook::default());
    Stats::load_modifier(&mut game.world, 0, "inspired", &modifier_data(None), None);
    let id = game.spawn(Position::ORIGIN, (Team::new(0), Modifiers::default()));
    let unit = game.entity(id);
    let inspired = game
        .world
        .resource::<ModifierBook>()
        .named(0, "inspired")
        .unwrap();
    // A capability lists the hold for one tick, as an area does: the tick holds it, and takes
    // the list, so the next tick, with nothing listed, lets go of it.
    let held = Held {
        target: id,
        modifier: inspired,
        source: None,
        ability: None,
        rank: Rank::FIRST,
    };
    game.world.resource_mut::<HeldModifiers>().0.push(held);
    game.step();
    let holds = |game: &TestMatch| {
        let modifiers = game.world.get::<Modifiers>(unit).unwrap();
        modifiers.get(inspired, None).is_some()
    };
    assert!(holds(&game));
    assert!(game.world.resource::<HeldModifiers>().0.is_empty());
    game.step();
    assert!(!holds(&game));
}

#[test]
fn a_restored_unit_derives_its_stats_and_tags_again() {
    // A walker of move speed 4, stunned by a modifier that grants tag 0, which blocks moving, and
    // slowed by another's cut of 0.5: move speed 4 × (1 − 0.5) = 2.
    let walker = stats(&[(Stat::Engine(EngineStat::MoveSpeed), Num::int(4), Num::ZERO)]);
    let stunned = Tag::new(0);
    let start = || {
        let mut game = stat_match(slice::from_ref(&walker));
        let effects = [(
            TagProperties::default().with_block(Block::Move),
            TagSet::default(),
        )];
        let book = TagBook::new(effects, [(UnitType::new(0), TagSet::default())]);
        game.world.insert_resource(book);
        // The type of the unit, 0, and the modifiers the shares below are instances of, 0 and 1.
        Units::load_type(
            &mut game.world,
            TypeScope::Mode,
            "walker",
            &UnitTypeData::default(),
        );
        let speed = game.world.resource::<StatBook>();
        let speed = speed.named(&Stat::Engine(EngineStat::MoveSpeed)).unwrap();
        let stunning = TagSet::of([stunned]);
        ModifierBook::push_changes(&mut game.world, &[(speed, StatOp::Add)], stunning);
        ModifierBook::push_changes(&mut game.world, &[(speed, StatOp::Cut)], TagSet::default());
        game
    };
    let mut game = start();
    let walker = unit(&mut game, 0);
    let speed = game.world.resource::<StatBook>();
    let speed = speed.named(&Stat::Engine(EngineStat::MoveSpeed)).unwrap();
    let modifiers = Modifiers::bundle([
        share(0, None, Num::ZERO, Reapply::Refresh),
        share(1, None, sixteenths(8), Reapply::Refresh),
    ]);
    game.world.entity_mut(walker).insert(modifiers);
    game.step();

    // Restored into a fresh match, the unit comes back with its state alone; after a tick on
    // both, its stats and tags are derived again as the original's are, and the hashes agree.
    let mut snapshot = Vec::new();
    game.registry.snapshot(&game.world, &mut snapshot);
    let mut restored = start();
    game.registry
        .restore(&snapshot, &mut restored.world)
        .unwrap();
    let id = *game.world.get::<StableId>(walker).unwrap();
    let copy = restored.world.resource::<EntityIndex>().get(id).unwrap();
    assert!(restored.world.get::<UnitStats>(copy).is_none());
    game.step();
    restored.world.run_schedule(SimUpdate);
    for (world, entity) in [(&game.world, walker), (&restored.world, copy)] {
        let stats = world.get::<UnitStats>(entity).unwrap();
        assert_eq!(stats.values()[speed.index()], Num::int(2));
        let tags = world.get::<UnitTags>(entity);
        assert!(UnitTags::properties_of(tags).blocks(Block::Move));
    }
    assert_eq!(
        game.registry.hash(&game.world),
        game.registry.hash(&restored.world)
    );
}

#[test]
fn a_modifiers_default_is_what_an_empty_table_reads() {
    assert_eq!(Toml::parse::<ModifierData>(""), Ok(ModifierData::default()));
}
