use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_abilities::*;
use campfire_actions::{
    ActionData, ActionKeys, ActionRange, CooldownHolder, DeliverySection, EffectData, EffectTo,
    KindSection, ListedEffect, PipelineField, RangeField, Targeting,
};
use campfire_capabilities::Match;
use campfire_capabilities::internals::*;
use campfire_combat::internals::Armed;
use campfire_combat::{AttackData, DamageEffectData, OnDeath, WeaponValue};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_production::TrainData;
use campfire_projectiles::ProjectileDeliveryData;
use campfire_script::ScriptLimits;
use campfire_sim::Capability;
use campfire_stats::{ModifierData, ModifierEffectData, StatChange, StatOp, Stats};
use campfire_units::Units;
use campfire_values::{
    FilterData, Number, PackageIndex, PackagePath, Param, ParamRef, Ranked, Scalar, Scaling, Stat,
};

/// Lash Out as the MOBA's Husk had it when these tests were written: the engine's tests keep
/// their own copy, so a balance change to the MOBA's hero changes none of them.
const LASH_OUT: &str = r#"
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.position, ctx.params.radius, "enemies") {
        ctx.damage(unit, "magic", ctx.params.damage);
    }
}

fn on_damage_taken(ctx, m, d) {
    if d.is_attack {
        ctx.cut_cooldown(m.carrier, "lash_out", ctx.params.cooldown_cut_ms);
    }
}
"#;

pub(crate) const fn int(value: i64) -> Number {
    Number::Value(Scalar::Int(value))
}

pub(crate) fn param(name: &str) -> Number {
    Number::Param(ParamRef {
        param: DeclaredName::new(name).unwrap(),
    })
}
/// Halves of a meter.
pub(crate) const fn half_meters(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 1))
}

/// A unit that strikes for `damage` within 2 m, as its windup of no ticks ends, every 5 ticks.
fn striker(damage: i64) -> Armed {
    Armed::melee(Num::int(500), Num::int(2), 0, 5, Num::int(damage)).on_death(OnDeath::Stay)
}

/// A cost of `amount` in pool `name`.
fn cost(name: &str, amount: Number) -> BTreeMap<DeclaredName, Ranked<Number>> {
    BTreeMap::from([(DeclaredName::new(name).unwrap(), Ranked::One(amount))])
}

/// Husk's Lash Out as its data declares it: no target, a cooldown of 10 s down to 6 s, 35 of the
/// caster's mana, and damage within 3.5 m of 75 to 175, plus half the caster's ability power.
fn lash_out() -> ActionData {
    ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("scripts/lash_out.rhai").unwrap()),
            cooldown_ms: Some(Ranked::PerRank(
                [10_000, 9000, 8000, 7000, 6000].map(int).to_vec(),
            )),
            cost: cost("mana", int(35)),
            params: BTreeMap::from([
                (
                    DeclaredName::new("radius").unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Decimal(half_meters(7)))),
                ),
                (
                    DeclaredName::new("damage").unwrap(),
                    Param::Scaling(Scaling {
                        base: Ranked::PerRank([75, 100, 125, 150, 175].map(Num::int).to_vec()),
                        per_level: Num::ZERO,
                        bonus: BTreeMap::new(),
                        ratios: [(Stat::named("ability_power").unwrap(), half_meters(1))].into(),
                    }),
                ),
                (
                    DeclaredName::new("cooldown_cut_ms").unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Int(500))),
                ),
            ]),
            ..ActionData::of(Targeting::None, CastData::default()).keys
        },
        ..ActionData::of(Targeting::None, CastData::default())
    }
}

/// A weapon of the rate of its unit's armor stat, and of its stat `attack_damage`, of physical
/// damage.
fn stat_attack() -> AttackData {
    AttackData {
        rate: WeaponValue::Stat(Stat::named("armor").unwrap()),
        rate_bonus: None,
        range_bonus: None,
        damage: WeaponValue::Stat(Stat::named("attack_damage").unwrap()),
        damage_bonus: None,
        damage_kind: DeclaredName::new("physical").unwrap(),
        prefers: None,
        clip: None,
        splash: None,
        scatter: None,
        pitch_deg: None,
        speed: None,
        streak: None,
        pre_attack: None,
        aim_deg: None,
    }
}

#[test]
fn a_kinds_section_names_the_first_pipeline_field_it_misuses() {
    // Strike is a cast: its script, cooldown, cost and params are a cast's to take.
    let mut action = Abilities::strike();
    assert_eq!(action.misused(), None);
    let attack = stat_attack();
    let train = TrainData {
        unit_type: DeclaredName::new("grunt").unwrap(),
        requires: None,
        at_place: false,
    };
    let of = |action: &ActionData, section: Arc<dyn KindSection>| ActionData {
        section,
        ..action.clone()
    };
    // As an attack, its script comes first of what an attack never takes; as a train, also its
    // script, before the range a train never takes.
    let attacking = |action: &ActionData| of(action, Arc::new(attack.clone()));
    let training = |action: &ActionData| of(action, Arc::new(train.clone()));
    assert_eq!(attacking(&action).misused(), Some(PipelineField::Script));
    assert_eq!(training(&action).misused(), Some(PipelineField::Script));
    // With no script or cooldown, an attack takes the rest, an `on_hit` list among them, and
    // never an `on_end` list.
    action.keys.script = None;
    action.keys.cooldown_ms = None;
    action.on_hit = sapper(false).on_hit;
    assert_eq!(attacking(&action).misused(), None);
    let ending = ActionData {
        on_end: action.on_hit.clone(),
        ..action.clone()
    };
    assert_eq!(attacking(&ending).misused(), Some(PipelineField::OnEnd));
    // A cast alone keeps its cooldown on its player (generals.md, A power's cooldown may be its
    // player's).
    let shared = ActionData {
        keys: ActionKeys {
            cooldown_holder: CooldownHolder::Player,
            ..action.clone().keys
        },
        ..action.clone()
    };
    assert_eq!(
        attacking(&shared).misused(),
        Some(PipelineField::CooldownHolder)
    );
    // An attack must have a range.
    let unranged = ActionData {
        keys: ActionKeys {
            range: None,
            ..action.clone().keys
        },
        ..action.clone()
    };
    assert_eq!(attacking(&unranged).misused(), Some(PipelineField::Range));
    // A train never takes a range, params or a hit list; with none, it takes the rest.
    assert_eq!(training(&action).misused(), Some(PipelineField::Range));
    let trained = ActionData {
        keys: ActionKeys {
            targeting: Targeting::None,
            range: None,
            params: BTreeMap::new(),
            ..action.keys
        },
        on_hit: Vec::new(),
        ..action
    };
    assert_eq!(training(&trained).misused(), None);
    assert_eq!(
        training(&trained).section::<TrainData>(),
        Some(&train),
        "a train's section is its capability's own type"
    );
    assert_eq!(training(&trained).section::<AttackData>(), None);
}

const STRIKE: &str =
    r#"fn on_resolve(ctx, caster, target) { ctx.damage(target, "true", ctx.params.damage); }"#;

fn abilities_match() -> Match {
    Abilities::match_of(
        ScriptLimits::ROOMY,
        &[Capability::Stats, Capability::Combat, Capability::Abilities],
    )
}

/// A match of stats, combat and abilities whose modifiers, by name in package 0, run
/// `source`, as `scripted` declares them.
fn abilities_match_with_modifiers(source: &str, modifiers: &[(&str, ModifierData)]) -> Match {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let script = Units::compile_hooked(&mut game, source).unwrap();
    let mut sorted = modifiers.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (name, data) in &sorted {
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, data, Some(script));
    }
    game
}

/// `effecting` to `to`.
pub(crate) const fn effect(effecting: Arc<dyn ListedEffect>, to: EffectTo) -> EffectData {
    EffectData {
        does: effecting,
        to,
    }
}

/// A modifier of no duration that runs `script`, with an interval of `interval_ms` and the
/// params `params`.
pub(crate) fn scripted(interval_ms: Option<i64>, params: &[(&str, i64)]) -> ModifierData {
    ModifierData {
        script: Some(PackagePath::parse("scripts/hooks.rhai").unwrap()),
        interval_ms: interval_ms.map(int),
        params: params
            .iter()
            .map(|&(name, value)| {
                (
                    DeclaredName::new(name).unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Int(value))),
                )
            })
            .collect(),
        ..ModifierData::default()
    }
}

/// A scaling table of `base` at every rank, `per_level`, and `ratios` and `bonus` by stat name.
fn scaling(
    base: Ranked<Num>,
    per_level: i64,
    ratios: &[(&str, Num)],
    bonus: &[(&str, Num)],
) -> Param {
    let by_name = |pairs: &[(&str, Num)]| {
        pairs
            .iter()
            .map(|&(name, ratio)| (Stat::named(name).unwrap(), ratio))
            .collect()
    };
    Param::Scaling(Scaling {
        base,
        per_level: Num::from_int(per_level).unwrap(),
        ratios: by_name(ratios),
        bonus: by_name(bonus),
    })
}

/// A weapon of no windup and 1 attack a second, of the attacker's attack damage, physical: within
/// 2 m, or within 6 m firing a `bolt`. Its `on_hit` shields the target by `mark`, of its `sap`
/// param of 40, then deals its `bite` param as true damage, 10 plus half the attacker's attack
/// damage.
fn sapper(ranged: bool) -> ActionData {
    let meters = if ranged { 6 } else { 2 };
    let delivery = ranged.then(|| -> Arc<dyn DeliverySection> {
        Arc::new(ProjectileDeliveryData::one(
            DeclaredName::new("bolt").unwrap(),
        ))
    });
    let bite = scaling(
        Ranked::One(Num::int(10)),
        0,
        &[("attack_damage", Num::HALF)],
        &[],
    );
    ActionData {
        keys: ActionKeys {
            script: None,
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(meters),
            )))),
            params: BTreeMap::from([
                (
                    DeclaredName::new("sap").unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Int(40))),
                ),
                (DeclaredName::new("bite").unwrap(), bite),
            ]),
            ..ActionData::of(
                Targeting::Unit(FilterData::parse("enemies").unwrap()),
                CastData::default(),
            )
            .keys
        },
        section: Arc::new(stat_attack()),
        delivery,
        on_hit: vec![
            effect(
                Arc::new(ModifierEffectData {
                    id: DeclaredName::new("mark").unwrap(),
                    duration_ms: None,
                }),
                EffectTo::Reached,
            ),
            effect(
                Arc::new(DamageEffectData {
                    amount: param("bite"),
                    kind: DeclaredName::new("true").unwrap(),
                }),
                EffectTo::Reached,
            ),
        ],
        ..ActionData::of(
            Targeting::Unit(FilterData::parse("enemies").unwrap()),
            CastData::default(),
        )
    }
}

/// A modifier of no script that changes `stats`, each by an add, and reads `params`, lasting
/// `duration_ms` when given.
pub(crate) fn changing(
    stats: &[(&str, Number)],
    params: &[(&str, Param)],
    duration_ms: Option<Number>,
) -> ModifierData {
    let change = |value: &Number| StatChange {
        op: StatOp::Add,
        value: value.clone(),
    };
    ModifierData {
        duration_ms,
        stats: stats
            .iter()
            .map(|(name, value)| (Stat::named(name).unwrap(), change(value)))
            .collect(),
        params: params
            .iter()
            .map(|(name, param)| (DeclaredName::new(name).unwrap(), param.clone()))
            .collect(),
        ..ModifierData::default()
    }
}

mod casts;
mod loads;
mod modifiers;
mod params;
mod reach;
mod timing;
mod weapons;
