use std::collections::BTreeMap;
use std::mem;
use std::num::NonZeroU32;
use std::slice;

use campfire_math::Vec3;
use campfire_sim::{Capability, IdAllocator, SimUpdate, TickRate};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::scripts::match_scripts::internals;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::modifier_data::{AuraData, Reapply};
use crate::stats::modifiers::{Application, Instance, StatShare};
use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_data::{StatValue, StatsData};
use crate::units::Units;
use crate::units::block::Block;
use crate::units::tag::Tag;
use crate::units::tag_effects::TagEffects;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::Number;
use crate::values::scalar::Scalar;

/// 30 ticks a second, as the MOBA runs.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// `value` sixteenths.
fn sixteenths(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 4))
}

/// The rules of the test: the engine's stats, move speed at most 5, `armor` at least −30, and the
/// pools' stats.
fn rules() -> BTreeMap<Stat, StatRule> {
    let free = StatRule::default();
    let speed = StatRule {
        min: None,
        max: Some(num(5)),
    };
    let armor = StatRule {
        min: Some(num(-30)),
        max: None,
    };
    [
        (EngineStat::MoveSpeed, speed),
        (EngineStat::AttackSpeed, free),
        (EngineStat::AttackDamage, free),
    ]
    .map(|(stat, rule)| (Stat::Engine(stat), rule))
    .into_iter()
    .chain([(armor_stat(), armor)])
    .chain(["health", "health_regen", "mana"].map(|name| (stat(name), free)))
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
                    base: Scalar::Decimal(*base),
                    per_level: Some(Scalar::Decimal(*per_level)),
                };
                (stat.clone(), value)
            })
            .collect(),
    )
}

/// A match with the stats capability and a book of `types`, with a move speed cap of 6.
fn stat_match(types: &[StatsData]) -> TestMatch {
    let mut game = TestMatch::new(&[Capability::Stats], RATE, None);
    let types = types.iter().enumerate().map(|(at, data)| {
        let unit_type = UnitType::new(u16::try_from(at).unwrap());
        (unit_type, data)
    });
    let book = StatBook::new(&rules(), types, RATE, num(6)).unwrap();
    let pools = PoolBook::new(&pools(), &book);
    Stats::load(&mut game.world, book, pools);
    game.world.add_schedule(mem::take(&mut game.schedule));
    game
}

/// A unit of `unit_type` at level 1 that walks, attacks with a windup of 2 ticks, and has a
/// pool of health and of mana, its values before the stats derive them.
fn unit(game: &mut TestMatch, unit_type: u16) -> Entity {
    let attack = AttackStats::new(Num::ONE, Ticks::new(2), Ticks::new(3), Num::ZERO).unwrap();
    let id = game.world.resource_mut::<IdAllocator>().allocate();
    game.world
        .spawn((
            id,
            UnitType::new(unit_type),
            Level::default(),
            UnitStats::default(),
            UnitTags::default(),
            MoveStep::new(Num::ZERO).unwrap(),
            attack,
            Pools::new([(HEALTH, Num::ONE), (MANA, Num::ONE)]).unwrap(),
        ))
        .id()
}

#[test]
fn a_units_stats_follow_its_type_and_level_within_their_limits() {
    // A hero of health 380 + 76 a level, regen 1.5, mana 250 + 45, attack damage 51 + 3,
    // attack speed 0.625 + 0.0625 and move speed 4 + 0.25.
    let engine = |stat: EngineStat| Stat::Engine(stat);
    let hero = stats(&[
        (stat("health"), num(380), num(76)),
        (stat("health_regen"), sixteenths(24), Num::ZERO),
        (stat("mana"), num(250), num(45)),
        (engine(EngineStat::AttackDamage), num(51), num(3)),
        (
            engine(EngineStat::AttackSpeed),
            sixteenths(10),
            sixteenths(1),
        ),
        (engine(EngineStat::MoveSpeed), num(4), sixteenths(4)),
    ]);
    let mut game = stat_match(&[hero]);
    let units = [unit(&mut game, 0)];
    game.world.run_schedule(SimUpdate);
    let get = |game: &TestMatch| {
        let world = &game.world;
        let unit = world.entity(units[0]);
        (
            unit.get::<MoveStep>().unwrap().get(),
            unit.get::<AttackStats>().unwrap().period(),
            unit.get::<AttackStats>().unwrap().damage(),
            *unit.get::<Pools>().unwrap(),
        )
    };
    let amounts = |pools: Pools, pool| (pools.current(pool).unwrap(), pools.max(pool).unwrap());
    // At level 1: 4 m/s, 4 × 2²⁴ ÷ 30 = 2 236 962.13 bits a tick, to 2 236 962; 0.625 attacks a
    // second, 30 ÷ 0.625 = 48 ticks; damage 51; full health of 380 and mana of 250.
    let (step, period, damage, pools) = get(&game);
    assert_eq!(step, Num::from_bits(2_236_962));
    assert_eq!((period, damage), (Ticks::new(48), num(51)));
    assert_eq!(amounts(pools, HEALTH), (num(380), num(380)));
    assert_eq!(amounts(pools, MANA), (num(250), num(250)));

    // Down 80 to 300 of 380, then at level 18: health 380 + 76 × 17 = 1672, the pool up by the
    // 1292 the maximum rose, to 1592, plus the regen of the tick that runs, 1.5 ÷ 30 = 0.05,
    // 838 860.8 bits, to 838 860 and its fifth carried; attack speed 0.625 + 17 × 0.0625 =
    // 1.6875, 30 ÷ 1.6875 = 17.8 ticks, to 18; damage 51 + 51 = 102; move speed 4 + 4.25 = 8.25,
    // past the limit of 5, 5 × 2²⁴ ÷ 30 = 2 796 202.67, to 2 796 203. Mana, full and with no
    // regen, rises with its maximum, to 250 + 45 × 17 = 1015.
    let mut pools = game.world.get_mut::<Pools>(units[0]).unwrap();
    pools.take(HEALTH, num(80));
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::new(18).unwrap();
    game.world.run_schedule(SimUpdate);
    let (step, period, damage, pools) = get(&game);
    assert_eq!(step, Num::from_bits(2_796_203));
    assert_eq!((period, damage), (Ticks::new(18), num(102)));
    let regen = Num::from_bits(838_860);
    assert_eq!(amounts(pools, HEALTH), (num(1592) + regen, num(1672)));
    assert_eq!(amounts(pools, MANA), (num(1015), num(1015)));

    // Over the next 29 ticks the regen adds the rest of the second: 30 ticks gain exactly 1.5.
    for _ in 0..29 {
        game.world.run_schedule(SimUpdate);
    }
    let pools = game.world.get::<Pools>(units[0]).unwrap();
    assert_eq!(pools.current(HEALTH), Some(num(1592) + sixteenths(24)));

    // Back to level 1: the maximum falls to 380, and the current amount to it.
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::default();
    game.world.run_schedule(SimUpdate);
    let (_, _, _, pools) = get(&game);
    assert_eq!(amounts(pools, HEALTH), (num(380), num(380)));
}

/// An application from `source` of modifier `id`, changing `stat` by `value` a stack with `op`,
/// refreshed or stacked as `reapply` says.
fn share(
    id: u16,
    source: Option<StableId>,
    stat: u16,
    op: StatOp,
    value: Num,
    reapply: Reapply,
) -> Application {
    Application {
        instance: Instance {
            id: ModifierId::new(id),
            source,
            ability: None,
            rank: 1,
            passive: false,
            held: false,
            aura_radius: None,
            stacks: 1,
            until: None,
            stack_life: None,
            stack_ends: Vec::new(),
            interval: None,
            shield: None,
            stats: vec![StatShare {
                stat,
                op,
                value,
                live: None,
            }],
            tags: TagSet::default(),
            state: Vec::new(),
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
                base: Scalar::Int(10),
                per_level: Some(Scalar::Int(2)),
            },
        )]
        .into(),
    );
    let tenths = |tenths: i64| num(tenths) / 10;
    let value = |changes: &[(StatOp, Num, u32)]| {
        let mut game = stat_match(slice::from_ref(&armored));
        let armor = game
            .world
            .resource::<StatBook>()
            .index(&armor_stat())
            .unwrap();
        let mut modifiers = Modifiers::default();
        for (at, &(op, value, stacks)) in changes.iter().enumerate() {
            let id = u16::try_from(at).unwrap();
            for _ in 0..stacks {
                modifiers.apply(share(id, None, armor, op, value, Reapply::Stack));
            }
        }
        let armored_unit = unit(&mut game, 0);
        let mut entity = game.world.entity_mut(armored_unit);
        entity.insert((Level::new(3).unwrap(), modifiers));
        game.world.run_schedule(SimUpdate);
        let stats = game.world.get::<UnitStats>(armored_unit).unwrap();
        stats.values()[usize::from(armor)]
    };
    assert_eq!(value(&[]), num(14));
    // Adds of 6 and of −1 three times: 17. Pcts of 0.3, 5 033 164.8 bits to 5 033 165, and of
    // −0.1, 1 677 721.6 to 1 677 722: 2²⁴ + 3 355 443 = 20 132 659 bits. Cuts of 0.1 and 0.25,
    // only the larger counting: 0.75. 17 × 20 132 659 × 0.75 = 256 691 402.25 bits, rounded once
    // to 256 691 402.
    let mixed = [
        (StatOp::Add, num(6), 1),
        (StatOp::Add, num(-1), 3),
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
        (StatOp::Add, num(-20), 1),
        (StatOp::Pct, sixteenths(8), 1),
        (StatOp::Cut, sixteenths(4), 1),
    ];
    assert_eq!(value(&negative), -num(6) - sixteenths(12));
    let floored = [(StatOp::Add, num(-40), 1), (StatOp::Pct, sixteenths(8), 1)];
    assert_eq!(value(&floored), num(-30));
}

#[test]
fn modifiers_change_a_units_stats_and_tags_hold_them_without_effect() {
    // Move speed 4. The tags: 0 detects, and is the type's own; 1, `slowed`, has no effect; 2,
    // `slow_immune`, makes immune to 1.
    let walker = stats(&[(Stat::Engine(EngineStat::MoveSpeed), num(4), Num::ZERO)]);
    let mut game = stat_match(&[walker]);
    let [sight, slowed, slow_immune] = [0, 1, 2].map(Tag::new);
    let effects = [
        (TagEffects::default().with_detects(), TagSet::default()),
        (TagEffects::default(), TagSet::default()),
        (TagEffects::default(), TagSet::of([slowed])),
    ];
    let own = TagSet::of([sight]);
    Units::load_tags(
        &mut game.world,
        TagBook::new(effects, [(UnitType::new(0), own)]),
    );
    let unit = unit(&mut game, 0);
    game.world.entity_mut(unit).insert(Modifiers::default());
    let book = game.world.resource::<StatBook>();
    let speed = book.index(&Stat::Engine(EngineStat::MoveSpeed)).unwrap();
    let mut ids = IdAllocator::default();
    let (first, second) = (Some(ids.allocate()), Some(ids.allocate()));
    let slowing = |source, value| {
        let mut application = share(0, source, speed, StatOp::Cut, value, Reapply::Refresh);
        application.instance.tags = TagSet::of([slowed]);
        application
    };
    // Cuts of 0.25 and 0.5 from two sources, each `slowed`: only the larger counts. An add of
    // 0.5 that stacks, twice: +1.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.apply(slowing(first, sixteenths(4)));
    modifiers.apply(slowing(second, sixteenths(8)));
    for _ in 0..2 {
        let add = share(1, first, speed, StatOp::Add, sixteenths(8), Reapply::Stack);
        modifiers.apply(add);
    }
    game.world.run_schedule(SimUpdate);
    // (4 + 1) × (1 − 0.5) = 2.5 m/s, 2.5 × 2²⁴ ÷ 30 = 1 398 101.33 bits a tick, to 1 398 101.
    let step = |game: &TestMatch| game.world.get::<MoveStep>(unit).unwrap().get();
    assert_eq!(step(&game), Num::from_bits(1_398_101));
    let tags = |game: &TestMatch| *game.world.get::<UnitTags>(unit).unwrap();
    assert_eq!(tags(&game).tags, TagSet::of([sight, slowed]));
    assert!(tags(&game).effects.detects());

    // A pct of 0.25: 5 × 1.25 × 0.5 = 3.125 m/s, 3.125 × 2²⁴ ÷ 30 = 1 747 626.67 bits, to
    // 1 747 627. An untagged cut of 1.5 counts as 1: no step at all.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.apply(share(
        2,
        first,
        speed,
        StatOp::Pct,
        sixteenths(4),
        Reapply::Refresh,
    ));
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(1_747_627));
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.apply(share(
        3,
        first,
        speed,
        StatOp::Cut,
        sixteenths(24),
        Reapply::Refresh,
    ));
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::ZERO);

    // The cut of 1.5 removed, slow immunity from a modifier of 2 stacks and no stat value holds
    // every `slowed` modifier without effect, so no cut counts: 5 × 1.25 = 6.25, past the limit
    // of 5: 5 × 2²⁴ ÷ 30 = 2 796 202.67 bits, to 2 796 203. `slowed` leaves the unit's tags, and
    // the immunity joins them.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.remove(ModifierId::new(3), first);
    let mut immune = share(4, first, speed, StatOp::Add, Num::ZERO, Reapply::Stack);
    immune.instance.tags = TagSet::of([slow_immune]);
    modifiers.apply(immune.clone());
    modifiers.apply(immune);
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(2_796_203));
    assert_eq!(tags(&game).tags, TagSet::of([sight, slow_immune]));
    assert_eq!(tags(&game).immune, TagSet::of([slowed]));

    // A modifier that grants both an immunity and the tag it is immune to holds itself: its
    // cut of 0.25 counts, whatever order the modifiers are in. 6.25 × 0.75 = 4.6875 m/s, 4.6875
    // × 2²⁴ ÷ 30 = 2 621 440 bits exactly.
    let mut both = slowing(second, sixteenths(4));
    both.instance.id = ModifierId::new(5);
    both.instance.tags = TagSet::of([slowed, slow_immune]);
    game.world.get_mut::<Modifiers>(unit).unwrap().apply(both);
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(2_621_440));
    assert_eq!(tags(&game).tags, TagSet::of([sight, slowed, slow_immune]));

    // The immunity ends: the held cuts act again, the larger 0.5: 3.125 m/s.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.remove(ModifierId::new(4), first);
    modifiers.remove(ModifierId::new(5), second);
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(1_747_627));
    assert_eq!(tags(&game).immune, TagSet::default());

    // Removing every modifier: back to 4 m/s, and to the type's tag alone.
    *game.world.get_mut::<Modifiers>(unit).unwrap() = Modifiers::default();
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(2_236_962));
    assert_eq!(tags(&game).tags, own);
}

#[test]
fn an_aura_holds_its_modifier_on_the_units_it_selects_within_its_radius() {
    let data = |aura: Option<AuraData>| ModifierData {
        script: None,
        duration_ms: None,
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        tags: Vec::new(),
        shield: None,
        aura,
        affects: None,
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    let limits = ScriptLimits {
        per_call: 10_000,
        player: 10_000,
        think: 10_000,
        mode: 10_000,
    };
    let scripts = internals::bare(limits, 1);
    let mut game = TestMatch::new(&[Capability::Stats], RATE, Some(scripts));
    let book = StatBook::new(&rules(), [], RATE, num(6)).unwrap();
    Stats::load(&mut game.world, book, PoolBook::default());
    // A presence of 2 m on allies, holding `inspired`.
    let presence = AuraData {
        radius: Number::Value(Scalar::Int(2)),
        affects: FilterData::parse("allies").unwrap(),
        modifier: "inspired".to_owned(),
    };
    Stats::load_modifier(&mut game.world, 0, "inspired", &data(None), None);
    Stats::load_modifier(&mut game.world, 0, "presence", &data(Some(presence)), None);
    game.world.add_schedule(mem::take(&mut game.schedule));
    let mut ids = IdAllocator::default();
    let at = |x: i64| Position::new(Vec3::new(num(x), Num::ZERO, Num::ZERO)).unwrap();
    let mut spawn = |game: &mut TestMatch, x: i64, team: u8| {
        let id = ids.allocate();
        let team = Team::new(team);
        game.world.spawn((id, at(x), team, Modifiers::default()));
        id
    };
    // The carrier at 0; an ally at 2, on the edge, and one at 3; an enemy at 1.
    let carrier = spawn(&mut game, 0, 0);
    let near = spawn(&mut game, 2, 0);
    let far = spawn(&mut game, 3, 0);
    let enemy = spawn(&mut game, 1, 1);
    let entity =
        |game: &TestMatch, id: StableId| game.world.resource::<EntityIndex>().get(id).unwrap();
    let [inspired, presence] = ["inspired", "presence"]
        .map(|name| game.world.resource::<ModifierBook>().find(0, name).unwrap());
    let mut presence_carrier = game
        .world
        .get_mut::<Modifiers>(entity(&game, carrier))
        .unwrap();
    presence_carrier.apply(Application {
        instance: Instance {
            id: presence,
            source: Some(carrier),
            ability: None,
            rank: 1,
            passive: false,
            held: false,
            aura_radius: Some(num(2)),
            stacks: 1,
            until: None,
            stack_life: None,
            stack_ends: Vec::new(),
            interval: None,
            shield: None,
            stats: Vec::new(),
            tags: TagSet::default(),
            state: Vec::new(),
        },
        reapply: Reapply::Refresh,
        max_stacks: None,
    });
    let holds = |game: &TestMatch, id: StableId| {
        let modifiers = game.world.get::<Modifiers>(entity(game, id)).unwrap();
        modifiers
            .get(inspired, Some(carrier))
            .is_some_and(|instance| instance.held && instance.until.is_none())
    };
    game.world.run_schedule(SimUpdate);
    // The carrier is its own ally, within 0 m of itself.
    assert_eq!(
        [carrier, near, far, enemy].map(|id| holds(&game, id)),
        [true, true, false, false]
    );
    // The near ally leaves, and the far one comes within 2 m.
    *game.world.get_mut::<Position>(entity(&game, near)).unwrap() = at(5);
    *game.world.get_mut::<Position>(entity(&game, far)).unwrap() = at(-2);
    game.world.run_schedule(SimUpdate);
    assert_eq!([near, far].map(|id| holds(&game, id)), [false, true]);
    // The carrier dies: its aura goes, with its own modifiers.
    let dead = entity(&game, carrier);
    game.world.entity_mut(dead).insert(Dead);
    game.world.run_schedule(SimUpdate);
    assert_eq!([carrier, far].map(|id| holds(&game, id)), [false, false]);
}

#[test]
fn a_restored_unit_derives_its_stats_and_tags_again() {
    // A walker of move speed 4, stunned by a modifier that grants tag 0, which blocks moving, and
    // slowed by another's cut of 0.5: move speed 4 × (1 − 0.5) = 2.
    let walker = stats(&[(Stat::Engine(EngineStat::MoveSpeed), num(4), Num::ZERO)]);
    let stunned = Tag::new(0);
    let start = || {
        let mut game = stat_match(slice::from_ref(&walker));
        let effects = [(
            TagEffects::default().with_block(Block::Move),
            TagSet::default(),
        )];
        let book = TagBook::new(effects, [(UnitType::new(0), TagSet::default())]);
        Units::load_tags(&mut game.world, book);
        game
    };
    let mut game = start();
    let walker = unit(&mut game, 0);
    let speed = game.world.resource::<StatBook>();
    let speed = speed.index(&Stat::Engine(EngineStat::MoveSpeed)).unwrap();
    let mut stun = share(0, None, speed, StatOp::Add, Num::ZERO, Reapply::Refresh);
    stun.instance.tags = TagSet::of([stunned]);
    let mut modifiers = Modifiers::default();
    modifiers.apply(stun);
    modifiers.apply(share(
        1,
        None,
        speed,
        StatOp::Cut,
        sixteenths(8),
        Reapply::Refresh,
    ));
    game.world.entity_mut(walker).insert(modifiers);
    game.world.run_schedule(SimUpdate);

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
    game.world.run_schedule(SimUpdate);
    restored.world.run_schedule(SimUpdate);
    for (world, entity) in [(&game.world, walker), (&restored.world, copy)] {
        let stats = world.get::<UnitStats>(entity).unwrap();
        assert_eq!(stats.values()[usize::from(speed)], num(2));
        let tags = world.get::<UnitTags>(entity);
        assert!(UnitTags::effects_of(tags).blocks(Block::Move));
    }
    assert_eq!(
        game.registry.hash(&game.world),
        game.registry.hash(&restored.world)
    );
}
