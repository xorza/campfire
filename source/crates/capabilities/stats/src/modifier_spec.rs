use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::Ticks;
use campfire_core::{Filter, ModifierId, StateValue, UnitTypes, UnknownName};
use campfire_math::Num;
use campfire_sim::TickRate;
use campfire_values::{DeclaredName, FilterData, Number, Stat};

use crate::error::{ModifierProblem, SpecError};
use crate::modifier_data::{ModifierData, Reapply};
use crate::modifier_state_field::ModifierStateField;
use crate::stat_id::StatId;
use crate::stat_op::StatOp;

/// A modifier as a match runs it: its data with every name resolved at load, its stats to their
/// places, its params to theirs, its filters to tags and its aura's modifier to its id, and each
/// fixed time to ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierSpec {
    /// Absent: until removed.
    pub duration: Option<SpecTime>,
    pub interval: Option<SpecTime>,
    pub stacks_expire: Option<SpecTime>,
    pub reapply: Reapply,
    /// Absent: no limit.
    pub max_stacks: Option<NonZeroU32>,
    /// Per stack, in the order of the stats.
    pub stats: Box<[SpecChange]>,
    /// The modifier ends when the shield is spent.
    pub shield: Option<SpecNumber>,
    pub aura: Option<AuraSpec>,
    /// The units of its player a player modifier holds on; absent, all of them.
    pub affects: Option<Filter>,
    /// Its script state's fields, in the order of their names, and each one's first value.
    pub fields: Arc<[ModifierStateField]>,
    pub initial: Box<[StateValue]>,
}

/// A change of the stat at `stat` by `op`, of `value` a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecChange {
    pub stat: StatId,
    pub op: StatOp,
    pub value: SpecNumber,
}

/// A modifier held on the units within `radius` that `affects` selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuraSpec {
    pub radius: SpecNumber,
    pub affects: Filter,
    pub modifier: ModifierId,
}

/// A number of a modifier: a value, or a param.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecNumber {
    Value(Num),
    Param(ParamPlace),
}

/// A time of a modifier: a fixed one, or a param of milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecTime {
    Fixed(Ticks),
    Param(ParamPlace),
}

/// Where a modifier's param is: at a place among its own params, or, when it declares none of
/// that name, the param `name` of the ability that applies it, which differs by ability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamPlace {
    Own(u16),
    Applier(DeclaredName),
}

/// What resolving a modifier's names reads: each stat's place, the tags of the match, the
/// tick rate, and its aura's modifier by name in its package.
#[derive(Debug)]
pub struct SpecNames<'a, S, M> {
    pub stat: S,
    pub types: &'a UnitTypes,
    pub rate: TickRate,
    pub modifier: M,
}

impl ModifierSpec {
    /// The spec of `data`, its names resolved by `names`; a value past what a number holds, a
    /// time too large to count in ticks, or a name the match does not have, fails.
    pub fn of<S, M>(
        data: &ModifierData,
        names: &SpecNames<'_, S, M>,
    ) -> Result<ModifierSpec, SpecError>
    where
        S: Fn(&Stat) -> Result<StatId, UnknownName>,
        M: Fn(&str) -> Result<ModifierId, UnknownName>,
    {
        let number = |number: &Number| SpecNumber::of(number, &data.params);
        let time = |field: &Option<Number>| {
            field
                .as_ref()
                .map(|field| SpecTime::of(field, &data.params, names.rate))
                .transpose()
        };
        let stats = data.stats.iter().map(|(stat, change)| {
            Ok::<_, SpecError>(SpecChange {
                stat: (names.stat)(stat)?,
                op: change.op,
                value: number(&change.value)?,
            })
        });
        let filter = |filter: &FilterData| Filter::resolve(filter, names.types);
        let aura = data.aura.as_ref().map(|aura| {
            Ok::<_, SpecError>(AuraSpec {
                radius: number(&aura.radius)?,
                affects: filter(&aura.affects)?,
                modifier: (names.modifier)(aura.modifier.as_str())?,
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
            affects: data.affects.as_ref().map(filter).transpose()?,
            fields: data
                .state
                .iter()
                .map(|(name, decl)| ModifierStateField {
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
