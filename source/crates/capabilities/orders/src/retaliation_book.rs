use bevy_ecs::resource::Resource;
use campfire_common::Ticks;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_math::Num;
use campfire_script::{Filter, MatchNames};
use campfire_units::UnitTypes;
use campfire_values::{PackageIndex, TimeTooLarge};

use crate::error::RetaliationError;
use crate::mode_retaliation_data::ModeRetaliationData;

/// The mode's `[retaliation]` as a match runs it, none for a mode of none (combat-response.md,
/// Retaliation is the mode's data). Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct RetaliationBook(pub(crate) Option<RetaliationRule>);

/// What a damaged unit answers and how: the aggressors' filter, as the damaged unit regards
/// them, the most meters it lies from one, edge to edge, the radius round it, past its body,
/// within which its allies answer, and the ticks they chase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetaliationRule {
    pub(crate) aggressors: Filter,
    pub(crate) distance: Num,
    pub(crate) radius: Num,
    pub(crate) chase: Ticks,
}

impl Book for RetaliationBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

impl RetaliationBook {
    /// The mode's `[retaliation]`, its filter resolved against the match's tags; an error for a
    /// tag the match lacks, a chase too long to count, or a distance or a radius below 0.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let Some(data) = cx.mode_section::<ModeRetaliationData>() else {
            return Ok(Self(None));
        };
        let at = || BookPlace::Section(ModeRetaliationData::KEY);
        let rule = |error| BookError::rule(PackageIndex::MODE, at(), error);
        let tags = cx.fact::<UnitTypes>()?.tags();
        let aggressors = Filter::resolve(&data.aggressors, tags)
            .map_err(|name| BookError::unknown(PackageIndex::MODE, at(), name))?;
        let chase = cx.rate().duration(data.chase_ms);
        let chase = chase.ok_or_else(|| rule(RetaliationError::Time(TimeTooLarge)))?;
        if data.distance < Num::ZERO || data.radius < Num::ZERO {
            return Err(rule(RetaliationError::Negative));
        }
        Ok(Self(Some(RetaliationRule {
            aggressors,
            distance: data.distance,
            radius: data.radius,
            chase,
        })))
    }
}
