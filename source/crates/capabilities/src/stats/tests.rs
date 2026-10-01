use std::collections::BTreeMap;
use std::mem;
use std::num::NonZeroU32;
use std::rc::Rc;

use bevy_ecs::entity::Entity;
use campfire_math::Vec3;
use campfire_sim::{Capability, IdAllocator, SimUpdate, TickRate};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::modifier_data::{AuraData, Reapply};
use crate::stats::modifiers::{Application, Instance, StatShare};
use crate::stats::stat::Stat;
use crate::stats::stat_rule::{Combine, StatRule};
use crate::stats::stats_data::{StatValue, StatsData};
use crate::units::Units;
use crate::units::tag::Tag;
use crate::units::tag_effects::TagEffects;
use crate::values::filter_data::FilterData;
use crate::values::number::Number;

/// 30 ticks a second, as the MOBA runs.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// `value` sixteenths.
fn sixteenths(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 4))
}

fn rule(combine: Combine, min: Option<Num>, max: Option<Num>) -> StatRule {
    StatRule { combine, min, max }
}

/// The rules of the test: the engine's stats, move speed at most 5, its rate at least −1, the
/// slow the highest from 0 to 0.99.
fn rules() -> BTreeMap<Stat, StatRule> {
    let sum = rule(Combine::Sum, None, None);
    let slow_max = Num::from_bits((99 << Num::FRAC_BITS) / 100);
    [
        (EngineStat::Health, sum),
        (EngineStat::HealthRegen, sum),
        (EngineStat::Resource, sum),
        (
            EngineStat::MoveSpeed,
            rule(Combine::Sum, None, Some(num(5))),
        ),
        (
            EngineStat::MoveSpeedPct,
            rule(Combine::Sum, Some(num(-1)), None),
        ),
        (
            EngineStat::Slow,
            rule(Combine::Highest, Some(Num::ZERO), Some(slow_max)),
        ),
        (EngineStat::AttackSpeed, sum),
        (EngineStat::AttackDamage, sum),
    ]
    .map(|(stat, rule)| (Stat::Engine(stat), rule))
    .into()
}

/// A unit type's stats, each a base and a gain a level.
fn stats(values: &[(EngineStat, Num, Num)]) -> StatsData {
    StatsData(
        values
            .iter()
            .map(|&(stat, base, per_level)| {
                let value = StatValue {
                    base: Scalar::Decimal(base),
                    per_level: Some(Scalar::Decimal(per_level)),
                };
                (Stat::Engine(stat), value)
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
    Stats::load(&mut game.world, book);
    game.world.add_schedule(mem::take(&mut game.schedule));
    game
}

/// A unit of `unit_type` at level 1 that walks, attacks with a windup of 2 ticks, and has a
/// pool of health and of resource, its values before the stats derive them.
fn unit(game: &mut TestMatch, unit_type: u16) -> Entity {
    let attack = AttackStats::new(Num::ONE, Ticks::new(2), Ticks::new(3), Num::ZERO).unwrap();
    game.world
        .spawn((
            UnitType::new(unit_type),
            Level::default(),
            UnitStats::default(),
            UnitTags::default(),
            MoveStep::new(Num::ZERO).unwrap(),
            attack,
            Health::new(Num::ONE).unwrap(),
            ResourcePool::new(Num::ONE).unwrap(),
        ))
        .id()
}

#[test]
fn a_units_stats_follow_its_type_and_level_within_their_limits() {
    // A hero of health 380 + 76 a level, regen 1.5, resource 250 + 45, attack damage 51 + 3,
    // attack speed 0.625 + 0.0625 and move speed 4 + 0.25.
    let hero = stats(&[
        (EngineStat::Health, num(380), num(76)),
        (EngineStat::HealthRegen, sixteenths(24), Num::ZERO),
        (EngineStat::Resource, num(250), num(45)),
        (EngineStat::AttackDamage, num(51), num(3)),
        (EngineStat::AttackSpeed, sixteenths(10), sixteenths(1)),
        (EngineStat::MoveSpeed, num(4), sixteenths(4)),
    ]);
    // A slow of 0.5 on move speed 4, and one of 1.5 the limit cuts to 0.99; a move speed rate
    // of −2 the limit raises to −1.
    let slowed = stats(&[
        (EngineStat::MoveSpeed, num(4), Num::ZERO),
        (EngineStat::Slow, sixteenths(8), Num::ZERO),
    ]);
    let stopped = stats(&[
        (EngineStat::MoveSpeed, num(4), Num::ZERO),
        (EngineStat::MoveSpeedPct, num(-2), Num::ZERO),
    ]);
    let mut game = stat_match(&[hero, slowed, stopped]);
    let units = [0, 1, 2].map(|unit_type| unit(&mut game, unit_type));
    game.world.run_schedule(SimUpdate);
    let get = |game: &TestMatch| {
        let world = &game.world;
        let unit = world.entity(units[0]);
        (
            unit.get::<MoveStep>().unwrap().get(),
            unit.get::<AttackStats>().unwrap().period(),
            unit.get::<AttackStats>().unwrap().damage(),
            *unit.get::<Health>().unwrap(),
            unit.get::<ResourcePool>().unwrap().max(),
        )
    };
    // At level 1: 4 m/s, 4 × 2²⁴ ÷ 30 = 2 236 962.13 bits a tick, to 2 236 962; 0.625 attacks a
    // second, 30 ÷ 0.625 = 48 ticks; damage 51; full health of 380 and resource of 250.
    let (step, period, damage, health, resource) = get(&game);
    assert_eq!(step, Num::from_bits(2_236_962));
    assert_eq!((period, damage), (Ticks::new(48), num(51)));
    assert_eq!(
        (health.current(), health.max(), resource),
        (num(380), num(380), num(250))
    );
    // The slowed one walks 4 × 0.5 = 2 m/s, 2 × 2²⁴ ÷ 30 = 1 118 481.07 bits, to 1 118 481; the
    // stopped one, at a rate of −1, not at all.
    let step_of = |game: &TestMatch, unit| game.world.get::<MoveStep>(unit).unwrap().get();
    assert_eq!(step_of(&game, units[1]), Num::from_bits(1_118_481));
    assert_eq!(step_of(&game, units[2]), Num::ZERO);

    // Down 80 to 300 of 380, then at level 18: health 380 + 76 × 17 = 1672, the pool up by the
    // 1292 the maximum rose, to 1592, plus the regen of the tick that runs, 1.5 ÷ 30 = 0.05,
    // 838 860.8 bits, to 838 860 and its fifth carried; attack speed 0.625 + 17 × 0.0625 =
    // 1.6875, 30 ÷ 1.6875 = 17.8 ticks, to 18; damage 51 + 51 = 102; move speed 4 + 4.25 = 8.25,
    // past the limit of 5, 5 × 2²⁴ ÷ 30 = 2 796 202.67, to 2 796 203.
    let mut health = game.world.get_mut::<Health>(units[0]).unwrap();
    health.take(num(80));
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::new(18).unwrap();
    game.world.run_schedule(SimUpdate);
    let (step, period, damage, health, resource) = get(&game);
    assert_eq!(step, Num::from_bits(2_796_203));
    assert_eq!((period, damage), (Ticks::new(18), num(102)));
    let regen = Num::from_bits(838_860);
    assert_eq!(
        (health.current(), health.max()),
        (num(1592) + regen, num(1672))
    );
    assert_eq!(resource, num(250 + 45 * 17));

    // Over the next 29 ticks the regen adds the rest of the second: 30 ticks gain exactly 1.5.
    for _ in 0..29 {
        game.world.run_schedule(SimUpdate);
    }
    let health = game.world.get::<Health>(units[0]).unwrap();
    assert_eq!(health.current(), num(1592) + sixteenths(24));

    // Back to level 1: the maximum falls to 380, and the current amount to it.
    *game.world.get_mut::<Level>(units[0]).unwrap() = Level::default();
    game.world.run_schedule(SimUpdate);
    let (_, _, _, health, _) = get(&game);
    assert_eq!((health.current(), health.max()), (num(380), num(380)));
}

/// An application from `source` of modifier `id`, adding `value` of `stat` a stack, refreshed or
/// stacked as `reapply` says.
fn share(
    id: u16,
    source: Option<StableId>,
    stat: u16,
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
            aura: false,
            aura_radius: None,
            stacks: 1,
            until: None,
            stack_life: None,
            stack_ends: Vec::new(),
            interval: None,
            shield: None,
            stats: vec![StatShare { stat, value }],
            tags: TagSet::default(),
            state: Vec::new(),
        },
        reapply,
        max_stacks: None,
    }
}

#[test]
fn modifiers_add_to_a_units_stats_by_each_stats_rule() {
    // Move speed 4. Its place among the stats: health, health regen, resource, move speed, its
    // rate, slow, attack speed, attack damage, in the engine's order. The tags: 0 detects, and is
    // the type's own; 1, `slowed`, has no effect; 2, `slow_immune`, makes immune to 1.
    let walker = stats(&[(EngineStat::MoveSpeed, num(4), Num::ZERO)]);
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
    let [speed, slow] = [EngineStat::MoveSpeed, EngineStat::Slow]
        .map(|stat| book.index(&Stat::Engine(stat)).unwrap());
    let mut ids = IdAllocator::default();
    let (first, second) = (Some(ids.allocate()), Some(ids.allocate()));
    let slowing = |source, value| {
        let mut application = share(0, source, slow, value, Reapply::Refresh);
        application.instance.tags = TagSet::of([slowed]);
        application
    };
    // Slows of 0.25 and 0.5 from two sources, each `slowed`: only the highest counts. A bonus of
    // 0.5 move speed that stacks, twice: +1.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.apply(slowing(first, sixteenths(4)));
    modifiers.apply(slowing(second, sixteenths(8)));
    for _ in 0..2 {
        modifiers.apply(share(1, first, speed, sixteenths(8), Reapply::Stack));
    }
    game.world.run_schedule(SimUpdate);
    // (4 + 1) × (1 − 0.5) = 2.5 m/s, 2.5 × 2²⁴ ÷ 30 = 1 398 101.33 bits a tick, to 1 398 101.
    let step = |game: &TestMatch| game.world.get::<MoveStep>(unit).unwrap().get();
    assert_eq!(step(&game), Num::from_bits(1_398_101));
    let tags = |game: &TestMatch| *game.world.get::<UnitTags>(unit).unwrap();
    assert_eq!(tags(&game).tags, TagSet::of([sight, slowed]));
    assert!(tags(&game).effects.detects());

    // A bonus of 3 more, past the limit of 5: 5 × 0.5 = 2.5 m/s again; a slow of 1.5, past its
    // limit of 0.99: 5 × 0.01 = 0.05 m/s, 0.05 × 2²⁴ ÷ 30 = 27 962.03 bits, to 27 962, the
    // slow's 0.99 itself a whole number of bits, 16 609 443, so 5 × (2²⁴ − 16 609 443) ÷ 30.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.apply(share(2, first, speed, num(3), Reapply::Refresh));
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(1_398_101));
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.apply(share(3, first, slow, sixteenths(24), Reapply::Refresh));
    game.world.run_schedule(SimUpdate);
    let kept = (1_i64 << 24) - (99 << 24) / 100;
    assert_eq!(step(&game), Num::from_bits((5 * kept + 15) / 30));

    // The untagged slow of 0.99 removed, slow immunity from a modifier of 2 stacks and no stat
    // value holds every `slowed` modifier without effect, so the slow counts as 0: 5 m/s, 5 ×
    // 2²⁴ ÷ 30 = 2 796 202.67 bits, to 2 796 203. `slowed` leaves the unit's tags, and the
    // immunity joins them.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.remove(ModifierId::new(3), first);
    let mut immune = share(4, first, speed, Num::ZERO, Reapply::Stack);
    immune.instance.tags = TagSet::of([slow_immune]);
    modifiers.apply(immune.clone());
    modifiers.apply(immune);
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(2_796_203));
    assert_eq!(tags(&game).tags, TagSet::of([sight, slow_immune]));
    assert_eq!(tags(&game).immune, TagSet::of([slowed]));

    // A modifier that grants both an immunity and the tag it is immune to holds itself: its
    // slow of 0.25 counts, whatever order the modifiers are in. 5 × 0.75 = 3.75 m/s, 3.75 × 2²⁴
    // ÷ 30 = 2 097 152 bits exactly.
    let mut both = slowing(second, sixteenths(4));
    both.instance.id = ModifierId::new(5);
    both.instance.tags = TagSet::of([slowed, slow_immune]);
    game.world.get_mut::<Modifiers>(unit).unwrap().apply(both);
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(2_097_152));
    assert_eq!(tags(&game).tags, TagSet::of([sight, slowed, slow_immune]));

    // The immunity ends: the held slows act again, the highest 0.5: 2.5 m/s.
    let mut modifiers = game.world.get_mut::<Modifiers>(unit).unwrap();
    modifiers.remove(ModifierId::new(4), first);
    modifiers.remove(ModifierId::new(5), second);
    game.world.run_schedule(SimUpdate);
    assert_eq!(step(&game), Num::from_bits(1_398_101));
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
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    let limits = ScriptLimits {
        per_call: 10_000,
        player: 10_000,
        think: 10_000,
        mode: 10_000,
    };
    let scripts = MatchScripts {
        limits,
        players: 1,
        damage_kinds: Rc::from([]),
    };
    let mut game = TestMatch::new(&[Capability::Stats], RATE, Some(scripts));
    let book = StatBook::new(&rules(), [], RATE, num(6)).unwrap();
    Stats::load(&mut game.world, book);
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
            aura: false,
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
            .is_some_and(|instance| instance.aura && instance.until.is_none())
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
