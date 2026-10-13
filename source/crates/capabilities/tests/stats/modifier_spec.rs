use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_capabilities::Release;
use campfire_capabilities::r#match::Match;
use campfire_common::Ticks;
use campfire_core::{Filter, ModifierId, NameKind, UnitTypes, UnknownName};
use campfire_math::Num;
use campfire_stats::*;
use campfire_values::{DeclaredName, FilterData, Number, Param, ParamRef, Ranked, Scalar, Stat};

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

fn param(text: &str) -> Number {
    Number::Param(ParamRef { param: name(text) })
}

#[test]
fn a_spec_resolves_every_name_of_its_modifier_at_load() {
    let mut types = UnitTypes::new(Release::engine_tags());
    types.declare("rooted").unwrap();
    let one = Param::Ranked(Ranked::One(Scalar::Int(1)));
    let mut data = ModifierData {
        script: None,
        duration_ms: Some(param("bind_ms")),
        interval_ms: Some(Number::Value(Scalar::Int(1500))),
        stacks_expire_ms: Some(param("stun_ms")),
        reapply: Reapply::Stack,
        max_stacks: NonZeroU32::new(3),
        stats: BTreeMap::from([
            (
                Stat::named("armor").unwrap(),
                StatChange {
                    op: StatOp::Add,
                    value: Number::Value(Scalar::Int(-3)),
                },
            ),
            (
                Stat::named("move_speed").unwrap(),
                StatChange {
                    op: StatOp::Pct,
                    value: param("slow"),
                },
            ),
        ]),
        tags: Vec::new(),
        shield: Some(Number::Value(Scalar::Int(40))),
        aura: Some(AuraData {
            radius: Number::Value(Scalar::Int(2)),
            affects: FilterData::parse("enemies:rooted").unwrap(),
            modifier: name("chill"),
        }),
        affects: Some(FilterData::parse("allies:!rooted").unwrap()),
        params: BTreeMap::from([(name("bind_ms"), one.clone()), (name("slow"), one)]),
        state: BTreeMap::new(),
    };
    let rate = Match::TEST_RATE;
    let stats = ["armor", "move_speed"].map(|stat| Stat::named(stat).unwrap());
    let stat = |stat: &Stat| {
        let at = stats.iter().position(|at| at == stat);
        at.map(StatId::new)
            .ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
    };
    let names = SpecNames {
        stat,
        types: &types,
        rate,
        modifier: |modifier: &str| {
            assert_eq!(modifier, "chill");
            Ok(ModifierId::new(7))
        },
    };
    let filter = |text| Filter::parse(text, &types).unwrap();
    // `bind_ms` and `slow` are its own, the first and second of its params by name;
    // `stun_ms` is the applier's. 1500 ms at 30 Hz is 1500 × 30 / 1000 = 45 ticks. The
    // changes follow the stats' order, an engine stat's first.
    let expected = ModifierSpec {
        duration: Some(SpecTime::Param(ParamPlace::Own(0))),
        interval: Some(SpecTime::Fixed(Ticks::new(45))),
        stacks_expire: Some(SpecTime::Param(ParamPlace::Applier(name("stun_ms")))),
        reapply: Reapply::Stack,
        max_stacks: NonZeroU32::new(3),
        stats: Box::new([
            SpecChange {
                stat: StatId::new(1),
                op: StatOp::Pct,
                value: SpecNumber::Param(ParamPlace::Own(1)),
            },
            SpecChange {
                stat: StatId::new(0),
                op: StatOp::Add,
                value: SpecNumber::Value(Num::from_int(-3).unwrap()),
            },
        ]),
        shield: Some(SpecNumber::Value(Num::from_int(40).unwrap())),
        aura: Some(AuraSpec {
            radius: SpecNumber::Value(Num::from_int(2).unwrap()),
            affects: filter("enemies:rooted"),
            modifier: ModifierId::new(7),
        }),
        affects: Some(filter("allies:!rooted")),
        fields: Arc::from([]),
        initial: Box::new([]),
    };
    assert_eq!(ModifierSpec::of(&data, &names), Ok(expected));
    assert_ne!(filter("enemies:rooted"), filter("allies:!rooted"));
    // 2³⁹ is past what a number holds, with 24 bits of fraction in 64; a negative time does
    // not count in ticks.
    data.shield = Some(Number::Value(Scalar::Int(1 << 39)));
    let overflow = Err(SpecError::Problem(ModifierProblem::Overflow));
    assert_eq!(ModifierSpec::of(&data, &names), overflow);
    data.shield = None;
    data.interval_ms = Some(Number::Value(Scalar::Int(-1)));
    let time = Err(SpecError::Problem(ModifierProblem::Time));
    assert_eq!(ModifierSpec::of(&data, &names), time);
}

#[test]
fn a_spec_fails_on_a_name_the_match_does_not_have() {
    // The match has the stat armor, the tag rooted and the modifier chill.
    let mut types = UnitTypes::new(Release::engine_tags());
    types.declare("rooted").unwrap();
    let armor = Stat::named("armor").unwrap();
    let names = SpecNames {
        stat: |stat: &Stat| {
            let found = (*stat == armor).then(|| StatId::new(0));
            found.ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
        },
        types: &types,
        rate: Match::TEST_RATE,
        modifier: |modifier: &str| {
            let found = (modifier == "chill").then(|| ModifierId::new(0));
            found.ok_or_else(|| UnknownName::new(NameKind::Modifier, modifier))
        },
    };
    let change = StatChange {
        op: StatOp::Add,
        value: Number::Value(Scalar::Int(1)),
    };
    let aura = |modifier: &str| AuraData {
        radius: Number::Value(Scalar::Int(2)),
        affects: FilterData::parse("enemies").unwrap(),
        modifier: name(modifier),
    };
    let known = ModifierData {
        stats: BTreeMap::from([(armor.clone(), change.clone())]),
        aura: Some(aura("chill")),
        affects: Some(FilterData::parse("allies:rooted").unwrap()),
        ..ModifierData::default()
    };
    assert!(ModifierSpec::of(&known, &names).is_ok());
    let speed = Stat::named("attack_speed").unwrap();
    let cases = [
        ModifierData {
            aura: Some(aura("frost")),
            ..known.clone()
        },
        ModifierData {
            stats: BTreeMap::from([(speed, change)]),
            ..known.clone()
        },
        ModifierData {
            affects: Some(FilterData::parse("allies:frozen").unwrap()),
            ..known
        },
    ];
    let unknown = [
        (NameKind::Modifier, "frost"),
        (NameKind::Stat, "attack_speed"),
        (NameKind::Filter, "allies:frozen"),
    ];
    for (data, (of, name)) in cases.iter().zip(unknown) {
        let fails = Err(SpecError::Unknown(UnknownName::new(of, name)));
        assert_eq!(ModifierSpec::of(data, &names), fails);
    }
}
