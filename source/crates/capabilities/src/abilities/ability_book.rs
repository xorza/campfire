use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::TickRate;
use serde::{Deserialize, Serialize};

use crate::abilities::ability_data::{AbilityData, Range, RangeField};
use crate::abilities::error::{AbilityError, AbilityField};
use crate::abilities::frame::Frame;
use crate::units::filter::Filter;
use crate::units::hook::Hook;
use crate::units::number::Number;
use crate::units::param::Param;
use crate::units::ranked::Ranked;
use crate::units::scalar::Scalar;

/// The abilities a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does.
#[derive(Resource, Debug, Default)]
pub(crate) struct AbilityBook {
    abilities: Vec<Ability>,
}

/// An ability, by its place in the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AbilityId(u32);

/// An ability as a match runs it.
#[derive(Debug)]
pub(crate) struct Ability {
    pub(crate) aim: Aim,
    /// Its capability fields at each rank, from rank 1.
    pub(crate) ranks: Vec<RankValues>,
    /// The script, when it defines `on_cast`: a script may serve only the ability's modifiers.
    pub(crate) on_cast: Option<ScriptId>,
    /// In the order of their names.
    pub(crate) params: Vec<Param>,
}

/// What an ability aims at, as a match runs it: a unit target's filter resolved against the
/// match's unit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aim {
    None,
    Point,
    Direction,
    Unit(Filter),
}

/// An ability's capability fields at one rank, times in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RankValues {
    pub(crate) range: Range,
    pub(crate) cooldown: u64,
    /// In the caster's resource.
    pub(crate) cost: u64,
    pub(crate) cast_time: u64,
}

impl AbilityBook {
    /// Loads `data`, aiming at `aim`, with its fields at each rank `ranks` holds; see
    /// `Abilities::load`. `frame` takes its param names.
    pub(crate) fn load(
        &mut self,
        host: &mut ScriptHost,
        frame: &mut Frame,
        data: &AbilityData,
        source: Option<&str>,
        aim: Aim,
        ranks: Vec<RankValues>,
    ) -> Result<AbilityId, AbilityError> {
        let script = match (&data.script, source) {
            (Some(_), Some(source)) => Some(host.compile(source).map_err(AbilityError::Script)?),
            (None, None) => None,
            _ => return Err(AbilityError::ScriptMismatch),
        };
        let on_cast = Hook::OnCast;
        let on_cast =
            script.filter(|&script| host.defines(script, on_cast.name(), on_cast.params()));
        let id = AbilityId(u32::try_from(self.abilities.len()).expect("abilities fit u32"));
        frame.add_param_names(data.params.keys().map(String::as_str));
        self.abilities.push(Ability {
            aim,
            ranks,
            on_cast,
            params: data.params.values().cloned().collect(),
        });
        Ok(id)
    }

    pub(crate) fn get(&self, id: AbilityId) -> Option<&Ability> {
        self.abilities.get(id.index())
    }
}

impl AbilityId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl RankValues {
    /// The fields of `data` at each of its `ranks` ranks, times in ticks at `rate`; an error when
    /// a per-rank array has another length.
    pub(crate) fn all(
        data: &AbilityData,
        ranks: u8,
        rate: TickRate,
    ) -> Result<Vec<RankValues>, AbilityError> {
        if !data.check_ranks(usize::from(ranks)) {
            return Err(AbilityError::RankCount(ranks));
        }
        (1..=ranks)
            .map(|rank| RankValues::of(data, rank, rate))
            .collect()
    }

    /// The fields of `data` at `rank`, a `{ param }` read from the params at that rank: global
    /// reach and 0 for a field it lacks. A field must be a whole number of milliseconds or of
    /// the resource, or a range of meters that is not negative; and it may not read a scaling
    /// param, whose value is the caster's, not the ability's.
    fn of(data: &AbilityData, rank: u8, rate: TickRate) -> Result<RankValues, AbilityError> {
        let scalar = |field: AbilityField, number: &Number| match number {
            Number::Value(value) => Ok(*value),
            Number::Param(reference) => param_at(data, &reference.param, rank, field),
        };
        let whole = |field, ranked: Option<&Ranked<Number>>| -> Result<u64, AbilityError> {
            let Some(ranked) = ranked else {
                return Ok(0);
            };
            let number = ranked.get(rank).ok_or(AbilityError::Field(field))?;
            match scalar(field, number)? {
                Scalar::Int(value) => u64::try_from(value).ok(),
                Scalar::Decimal(_) => None,
            }
            .ok_or(AbilityError::Field(field))
        };
        let ticks = |ms: u64| rate.ticks(ms).ok_or(AbilityError::TimeTooLarge);
        let range = match &data.range {
            None => Range::Global,
            Some(ranked) => {
                let at = ranked.get(rank);
                match at.ok_or(AbilityError::Field(AbilityField::Range))? {
                    RangeField::Range(range) => *range,
                    RangeField::Param(reference) => {
                        let field = AbilityField::Range;
                        let meters = param_at(data, &reference.param, rank, field)?.to_num();
                        Range::Meters(
                            meters
                                .filter(|meters| *meters >= Num::ZERO)
                                .ok_or(AbilityError::Field(field))?,
                        )
                    }
                }
            }
        };
        Ok(RankValues {
            range,
            cooldown: ticks(whole(AbilityField::Cooldown, data.cooldown_ms.as_ref())?)?,
            cost: whole(AbilityField::Cost, data.cost.as_ref())?,
            cast_time: ticks(whole(AbilityField::CastTime, data.cast_time_ms.as_ref())?)?,
        })
    }
}

/// The param `name` of `data` at `rank`, for the capability field `field`.
fn param_at(
    data: &AbilityData,
    name: &str,
    rank: u8,
    field: AbilityField,
) -> Result<Scalar, AbilityError> {
    match data.params.get(name) {
        Some(Param::Ranked(ranked)) => ranked.at(rank).ok_or(AbilityError::Field(field)),
        Some(Param::Scaling(_)) | None => Err(AbilityError::Field(field)),
    }
}
