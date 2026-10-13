use thiserror::Error;

use crate::actions::action_data_field::ActionDataField;
use crate::actions::action_kind::ActionKind;
use crate::scripts::error::UnknownName;
use crate::scripts::hook::Hook;
use crate::values::declared_name::DeclaredName;
use crate::values::error::TimeTooLarge;

/// A capability field of an action that the release runs, which does not give a value of its
/// kind at a rank: a whole number of milliseconds or of a pool the mode declares, or a range of
/// meters that is not negative, and never through a scaling param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionField {
    Range,
    Cooldown,
    Cost,
    Windup,
    /// Its charges: a count of 1 to 255, and a recharge of whole milliseconds.
    Charges,
    /// Its toggle's cost: whole amounts of the caster's pools.
    Toggle,
    /// Its channel: a length and a time between ticks of whole milliseconds, neither 0.
    Channel,
    /// Its charge's most: whole milliseconds, not 0.
    Charge,
}

/// Why an action's data makes no action of a match: a rule of design 08's checks that its data
/// breaks, which the action's book holds as it builds. Packages are untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionProblem {
    /// A time too large to count in ticks at the match's rate.
    #[error(transparent)]
    Time(#[from] TimeTooLarge),
    /// A per-rank array has another length than the action's ranks, which it gives.
    #[error("a per-rank array of other than {0} ranks")]
    RankCount(u8),
    /// The slot kinds that hold it give it different ranks.
    #[error("placed in slot kinds of different ranks")]
    SlottedRanks,
    /// A capability field does not hold at a rank.
    #[error("its field {0:?} does not hold at a rank")]
    Field(ActionField),
    /// A field its kind needs is missing, or one its kind refuses is given.
    #[error("its kind needs or refuses `{}`", .0.name())]
    KindField(ActionDataField),
    /// A gather's `bounce` is no distance from 0.
    #[error("its bounce is no distance from 0")]
    Bounce,
    /// A build or a gather reaches globally at a rank, where it needs meters.
    #[error("its range is global at a rank, where its kind needs meters")]
    Global(ActionKind),
    /// The projectile type its delivery names lacks its `projectile` section.
    #[error("unit type {0} lacks the projectile section its delivery needs")]
    Section(DeclaredName),
    /// A build's unit type has no box body.
    #[error("its building has no box body")]
    NoBox,
    /// A build's site starts at no life: its `start_life` is 0, or missing for a building with
    /// the life pool.
    #[error("its building's site starts at no life")]
    StartLife,
    /// A number or a duration of an effect of its list `list` breaks `rule`.
    #[error("its {list:?} list: {rule}")]
    Effect { list: Hook, rule: EffectRule },
}

/// A rule a number of a listed effect breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EffectRule {
    /// A value, at some rank, past what a sim number holds.
    #[error("a number past what a sim number holds")]
    Overflow,
    /// A value, at some rank, below 0.
    #[error("a number below 0")]
    Negative,
    /// A duration that is no whole number of milliseconds within a `u32` at every rank, or that
    /// reads a scaling param.
    #[error("a duration of no whole milliseconds")]
    Duration,
}

/// Why an action's data does not load into a match: a rule it breaks, or a name the match does
/// not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ActionError {
    #[error(transparent)]
    Problem(#[from] ActionProblem),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}

impl From<TimeTooLarge> for ActionError {
    fn from(error: TimeTooLarge) -> ActionError {
        ActionError::Problem(ActionProblem::Time(error))
    }
}
