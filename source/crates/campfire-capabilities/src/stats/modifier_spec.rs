use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::Ticks;
use campfire_math::Num;
use campfire_sim::TickRate;

use crate::scripts::state_value::StateValue;
use crate::stats::error::ModifierProblem;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_handle::StateField;
use crate::stats::stat_id::StatId;
use crate::stats::stat_op::StatOp;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::number::Number;
use crate::values::stat::Stat;

/// A modifier as a match runs it: its data with every name resolved at load, its stats to their
/// places, its params to theirs, its filters to tags and its aura's modifier to its id, and each
/// fixed time to ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModifierSpec {
    /// Absent: until removed.
    pub(crate) duration: Option<SpecTime>,
    pub(crate) interval: Option<SpecTime>,
    pub(crate) stacks_expire: Option<SpecTime>,
    pub(crate) reapply: Reapply,
    /// Absent: no limit.
    pub(crate) max_stacks: Option<NonZeroU32>,
    /// Per stack, in the order of the stats.
    pub(crate) stats: Box<[SpecChange]>,
    /// The modifier ends when the shield is spent.
    pub(crate) shield: Option<SpecNumber>,
    pub(crate) aura: Option<AuraSpec>,
    /// The units of its player a player modifier holds on; absent, all of them.
    pub(crate) affects: Option<Filter>,
    /// Its script state's fields, in the order of their names, and each one's first value.
    pub(crate) fields: Arc<[StateField]>,
    pub(crate) initial: Box<[StateValue]>,
}

/// A change of the stat at `stat` by `op`, of `value` a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecChange {
    pub(crate) stat: StatId,
    pub(crate) op: StatOp,
    pub(crate) value: SpecNumber,
}

/// A modifier held on the units within `radius` that `affects` selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuraSpec {
    pub(crate) radius: SpecNumber,
    pub(crate) affects: Filter,
    pub(crate) modifier: ModifierId,
}

/// A number of a modifier: a value, or a param.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpecNumber {
    Value(Num),
    Param(ParamPlace),
}

/// A time of a modifier: a fixed one, or a param of milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpecTime {
    Fixed(Ticks),
    Param(ParamPlace),
}

/// Where a modifier's param is: at a place among its own params, or, when it declares none of
/// that name, the param `name` of the ability that applies it, which differs by ability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParamPlace {
    Own(u16),
    Applier(DeclaredName),
}

/// What resolving a modifier's names reads: each stat's place, the tags of the match, the
/// tick rate, and its aura's modifier by name in its package.
#[derive(Debug)]
pub(crate) struct SpecNames<'a, S, M> {
    pub(crate) stat: S,
    pub(crate) types: &'a UnitTypes,
    pub(crate) rate: TickRate,
    pub(crate) modifier: M,
}

impl ModifierSpec {
    /// The spec of `data`, its names resolved by `names`, which the package load checked; a
    /// value past what a number holds, or a time too large to count in ticks, fails.
    pub(crate) fn of<S, M>(
        data: &ModifierData,
        names: &SpecNames<'_, S, M>,
    ) -> Result<ModifierSpec, ModifierProblem>
    where
        S: Fn(&Stat) -> StatId,
        M: Fn(&str) -> ModifierId,
    {
        let number = |number: &Number| SpecNumber::of(number, &data.params);
        let time = |field: &Option<Number>| {
            field
                .as_ref()
                .map(|field| SpecTime::of(field, &data.params, names.rate))
                .transpose()
        };
        let stats = data.stats.iter().map(|(stat, change)| {
            Ok(SpecChange {
                stat: (names.stat)(stat),
                op: change.op,
                value: number(&change.value)?,
            })
        });
        let filter = |filter| {
            Filter::resolve(filter, names.types).expect("the load checked the modifier's filters")
        };
        let aura = data.aura.as_ref().map(|aura| {
            Ok::<_, ModifierProblem>(AuraSpec {
                radius: number(&aura.radius)?,
                affects: filter(&aura.affects),
                modifier: (names.modifier)(aura.modifier.as_str()),
            })
        });
        Ok(ModifierSpec {
            duration: time(&data.duration_ms)?,
            interval: time(&data.interval_ms)?,
            stacks_expire: time(&data.stacks_expire_ms)?,
            reapply: data.reapply,
            max_stacks: data.max_stacks,
            stats: stats.collect::<Result<_, _>>()?,
            shield: data.shield.as_ref().map(number).transpose()?,
            aura: aura.transpose()?,
            affects: data.affects.as_ref().map(filter),
            fields: data
                .state
                .iter()
                .map(|(name, decl)| StateField {
                    name: name.as_str().into(),
                    kind: decl.kind(),
                })
                .collect(),
            initial: data
                .state
                .values()
                .map(|decl| decl.initial().clone())
                .collect(),
        })
    }

    /// The places of the params its numbers read: its stat changes', its shield's and its aura's
    /// radius's.
    pub(crate) fn number_params(&self) -> impl Iterator<Item = &ParamPlace> {
        let changes = self.stats.iter().map(|change| &change.value);
        let numbers = changes
            .chain(&self.shield)
            .chain(self.aura.iter().map(|aura| &aura.radius));
        numbers.filter_map(|number| match number {
            SpecNumber::Param(place) => Some(place),
            SpecNumber::Value(_) => None,
        })
    }

    /// The places of the params its times read: its duration's, its interval's and its stacks'.
    pub(crate) fn time_params(&self) -> impl Iterator<Item = &ParamPlace> {
        let times = [&self.duration, &self.interval, &self.stacks_expire];
        times.into_iter().flatten().filter_map(|time| match time {
            SpecTime::Param(place) => Some(place),
            SpecTime::Fixed(_) => None,
        })
    }
}

impl SpecNumber {
    /// `number` of a modifier whose own params are `params`.
    fn of<V>(
        number: &Number,
        params: &BTreeMap<DeclaredName, V>,
    ) -> Result<SpecNumber, ModifierProblem> {
        match number {
            Number::Value(value) => value
                .to_num()
                .map(SpecNumber::Value)
                .ok_or(ModifierProblem::Overflow),
            Number::Param(reference) => {
                Ok(SpecNumber::Param(ParamPlace::of(&reference.param, params)))
            }
        }
    }
}

impl SpecTime {
    /// `number` of milliseconds of a modifier whose own params are `params`, a fixed one in
    /// ticks at `rate`.
    fn of<V>(
        number: &Number,
        params: &BTreeMap<DeclaredName, V>,
        rate: TickRate,
    ) -> Result<SpecTime, ModifierProblem> {
        match SpecNumber::of(number, params)? {
            SpecNumber::Value(ms) => Ok(SpecTime::Fixed(
                ModifierData::ticks(ms, rate).ok_or(ModifierProblem::Time)?,
            )),
            SpecNumber::Param(place) => Ok(SpecTime::Param(place)),
        }
    }

    /// Its ticks at `rate`, a param's read by `param`, which its way checked counts in ticks.
    pub(crate) fn ticks(&self, rate: TickRate, param: impl Fn(&ParamPlace) -> Num) -> Ticks {
        match self {
            SpecTime::Fixed(ticks) => *ticks,
            SpecTime::Param(place) => {
                ModifierData::ticks(param(place), rate).expect("its way checked the time")
            }
        }
    }
}

impl ParamPlace {
    /// The name of the applier's param it is; none for one of the modifier's own.
    pub(crate) const fn applier(&self) -> Option<&DeclaredName> {
        match self {
            ParamPlace::Own(_) => None,
            ParamPlace::Applier(name) => Some(name),
        }
    }

    /// Where the param `name` of a modifier whose own params are `params` is.
    fn of<V>(name: &DeclaredName, params: &BTreeMap<DeclaredName, V>) -> ParamPlace {
        match params.keys().position(|own| own == name) {
            Some(at) => ParamPlace::Own(u16::try_from(at).expect("params fit u16")),
            None => ParamPlace::Applier(name.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroU32;
    use std::sync::Arc;

    use crate::capability_set::test_match::TestMatch;
    use campfire_common::Ticks;
    use campfire_math::Num;

    use crate::stats::error::ModifierProblem;
    use crate::stats::modifier_data::{AuraData, ModifierData, Reapply};
    use crate::stats::modifier_spec::{
        AuraSpec, ModifierSpec, ParamPlace, SpecChange, SpecNames, SpecNumber, SpecTime,
    };
    use crate::stats::stat_change::StatChange;
    use crate::stats::stat_id::StatId;
    use crate::stats::stat_op::StatOp;
    use crate::units::filter::Filter;
    use crate::units::modifier_id::ModifierId;
    use crate::units::unit_types::UnitTypes;
    use crate::values::declared_name::DeclaredName;
    use crate::values::filter_data::FilterData;
    use crate::values::number::{Number, ParamRef};
    use crate::values::param::Param;
    use crate::values::ranked::Ranked;
    use crate::values::scalar::Scalar;
    use crate::values::stat::Stat;

    fn name(text: &str) -> DeclaredName {
        DeclaredName::new(text).unwrap()
    }

    fn param(text: &str) -> Number {
        Number::Param(ParamRef { param: name(text) })
    }

    #[test]
    fn a_spec_resolves_every_name_of_its_modifier_at_load() {
        let mut types = UnitTypes::default();
        types.declare("rooted");
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
        let rate = TestMatch::RATE;
        let stats = ["armor", "move_speed"].map(|stat| Stat::named(stat).unwrap());
        let names = SpecNames {
            stat: |stat: &Stat| StatId::new(stats.iter().position(|at| at == stat).unwrap()),
            types: &types,
            rate,
            modifier: |modifier: &str| {
                assert_eq!(modifier, "chill");
                ModifierId::new(7)
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
        assert_eq!(
            ModifierSpec::of(&data, &names),
            Err(ModifierProblem::Overflow)
        );
        data.shield = None;
        data.interval_ms = Some(Number::Value(Scalar::Int(-1)));
        assert_eq!(ModifierSpec::of(&data, &names), Err(ModifierProblem::Time));
    }
}
