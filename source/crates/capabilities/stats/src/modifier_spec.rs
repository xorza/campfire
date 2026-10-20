use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::{DeclaredName, Ticks};
use campfire_math::Num;
use campfire_script::{Audience, Filter, StateSchema, TagNames, UnknownName};
use campfire_sim::MatchRate;
use campfire_units::StatId;
use campfire_values::{FilterData, ModifierId, Names, Number, Param, Stat};

use crate::applier_param::ApplierParam;
use crate::error::{ModifierProblem, SpecError};
use crate::interval_revival::IntervalRevival;
use crate::modifier_data::{ModifierData, Reapply};
use crate::stat_op::StatOp;

/// A modifier as a match runs it: its data with every name resolved at load, its stats to their
/// places, its params to theirs, its filters to tags and its aura's modifier to its id, and each
/// fixed time to ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierSpec {
    /// Absent: until removed.
    pub duration: Option<SpecTime>,
    pub interval: Option<SpecTime>,
    pub revival: IntervalRevival,
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
    /// Its script state's fields, each of class `none`, as one run of values holds them.
    pub state: Arc<StateSchema>,
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
/// that name, the param of that name of the ability that applies it, which differs by ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamPlace {
    Own(u16),
    Applier(ApplierParam),
}

/// What a modifier's param names resolve against: its own params, and the names of the params
/// the match's modifiers read from their abilities.
#[derive(Debug)]
struct ParamPlaces<'a> {
    own: &'a BTreeMap<DeclaredName, Param>,
    appliers: &'a mut Names<ApplierParam>,
}

/// What resolving a modifier's names reads: each stat's place, the tags of the match, the
/// tick rate, and its aura's modifier by name in its package.
#[derive(Debug)]
pub struct SpecNames<'a, S, M> {
    pub stat: S,
    pub tags: &'a TagNames,
    pub tick_rate: MatchRate,
    pub modifier: M,
}

impl ModifierSpec {
    /// The spec of `data`, its names resolved by `names`, each param it reads from the ability
    /// that applies it named in `appliers`, which gains the names it lacks; a value past what a
    /// number holds, a time too large to count in ticks, or a name the match does not have, fails.
    pub fn of<S, M>(
        data: &ModifierData,
        names: &SpecNames<'_, S, M>,
        appliers: &mut Names<ApplierParam>,
    ) -> Result<Self, SpecError>
    where
        S: Fn(&Stat) -> Result<StatId, UnknownName>,
        M: Fn(&str) -> Result<ModifierId, UnknownName>,
    {
        let mut places = ParamPlaces {
            own: &data.params,
            appliers,
        };
        let mut time = |field: &Option<Number>| {
            field
                .as_ref()
                .map(|field| places.time(field, names.tick_rate))
                .transpose()
        };
        let duration = time(&data.duration_ms)?;
        let interval = time(&data.interval_ms)?;
        let stacks_expire = time(&data.stacks_expire_ms)?;
        let mut stats = Vec::with_capacity(data.stats.len());
        for (stat, change) in &data.stats {
            stats.push(SpecChange {
                stat: (names.stat)(stat)?,
                op: change.op,
                value: places.number(&change.value)?,
            });
        }
        let shield = data
            .shield
            .as_ref()
            .map(|shield| places.number(shield))
            .transpose()?;
        let filter = |filter: &FilterData| Filter::resolve(filter, names.tags);
        let aura = match &data.aura {
            Some(aura) => Some(AuraSpec {
                radius: places.number(&aura.radius)?,
                affects: filter(&aura.affects)?,
                modifier: (names.modifier)(aura.modifier.as_str())?,
            }),
            None => None,
        };
        Ok(Self {
            duration,
            interval,
            revival: data.interval_revival,
            stacks_expire,
            reapply: data.reapply,
            max_stacks: data.max_stacks,
            stats: stats.into_boxed_slice(),
            shield,
            aura,
            affects: data.affects.as_ref().map(filter).transpose()?,
            state: Arc::new(StateSchema::new(
                data.state
                    .iter()
                    .map(|(name, decl)| (name.as_str(), Audience::None, decl)),
            )),
        })
    }

    /// The places of the params its numbers read: its stat changes', its shield's and its aura's
    /// radius's.
    pub(crate) fn number_params(&self) -> impl Iterator<Item = ParamPlace> {
        let changes = self.stats.iter().map(|change| &change.value);
        let numbers = changes
            .chain(&self.shield)
            .chain(self.aura.iter().map(|aura| &aura.radius));
        numbers.filter_map(|number| match number {
            SpecNumber::Param(place) => Some(*place),
            SpecNumber::Value(_) => None,
        })
    }

    /// The places of the params its times read: its duration's, its interval's and its stacks'.
    pub(crate) fn time_params(&self) -> impl Iterator<Item = ParamPlace> {
        let times = [&self.duration, &self.interval, &self.stacks_expire];
        times.into_iter().flatten().filter_map(|time| match time {
            SpecTime::Param(place) => Some(*place),
            SpecTime::Fixed(_) => None,
        })
    }
}

impl SpecTime {
    /// Its ticks at `rate`, a param's read by `param`, which its way checked counts in ticks.
    pub(crate) fn ticks(&self, rate: MatchRate, param: impl Fn(ParamPlace) -> Num) -> Ticks {
        match self {
            Self::Fixed(ticks) => *ticks,
            Self::Param(place) => {
                ModifierData::ticks(param(*place), rate).expect("its way checked the time")
            }
        }
    }
}

impl ParamPlace {
    /// The applier's param it is; none for one of the modifier's own.
    pub(crate) const fn applier(self) -> Option<ApplierParam> {
        match self {
            Self::Own(_) => None,
            Self::Applier(param) => Some(param),
        }
    }
}

impl ParamPlaces<'_> {
    /// Where the param `name` is.
    fn place(&mut self, name: &DeclaredName) -> ParamPlace {
        match self.own.keys().position(|own| own == name) {
            Some(at) => ParamPlace::Own(u16::try_from(at).expect("params fit u16")),
            None => ParamPlace::Applier(
                self.appliers
                    .named(name.as_str())
                    .unwrap_or_else(|| self.appliers.add(name.as_str())),
            ),
        }
    }

    /// `number` of the modifier.
    fn number(&mut self, number: &Number) -> Result<SpecNumber, ModifierProblem> {
        match number {
            Number::Value(value) => value
                .to_num()
                .map(SpecNumber::Value)
                .ok_or(ModifierProblem::Overflow),
            Number::Param(reference) => Ok(SpecNumber::Param(self.place(&reference.param))),
        }
    }

    /// `number` of milliseconds of the modifier, a fixed one in ticks at `rate`.
    fn time(&mut self, number: &Number, rate: MatchRate) -> Result<SpecTime, ModifierProblem> {
        match self.number(number)? {
            SpecNumber::Value(ms) => Ok(SpecTime::Fixed(
                ModifierData::ticks(ms, rate).ok_or(ModifierProblem::Time)?,
            )),
            SpecNumber::Param(place) => Ok(SpecTime::Param(place)),
        }
    }
}
