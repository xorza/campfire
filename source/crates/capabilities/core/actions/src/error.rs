use campfire_script::UnknownName;
use campfire_values::{DeclaredName, TimeTooLarge};
use thiserror::Error;

use crate::action_data::error::{ActionField, EffectRule};
use crate::action_data_field::ActionDataField;
use crate::action_kind::{ActionKind, MeteredKind};
use crate::list_hook::ListHook;

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
    /// Its kind is none the release runs.
    #[error("its kind {0:?} is none the release runs")]
    NotRun(ActionKind),
    /// A use, a build or a gather reaches globally at a rank, where it needs meters.
    #[error("its range is global at a rank, where its kind needs meters")]
    Global(MeteredKind),
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
    Effect { list: ListHook, rule: EffectRule },
}

/// Why the mode's slot kinds do not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SlotKindsError {
    /// More kinds than a slot kind's id counts.
    #[error("more slot kinds than a slot kind's id counts")]
    TooMany,
    #[error("the slot kind `{0}` twice")]
    Repeated(DeclaredName),
}

/// Why an action's data does not load into a match: a rule it breaks, or a name the match does
/// not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionError {
    #[error(transparent)]
    Problem(#[from] ActionProblem),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}

impl From<TimeTooLarge> for ActionError {
    fn from(error: TimeTooLarge) -> Self {
        Self::Problem(ActionProblem::Time(error))
    }
}

impl From<KindError> for ActionError {
    fn from(error: KindError) -> Self {
        Self::Problem(error.into())
    }
}

/// Why an action's data gives no `KindData`: its kind is none the release runs, or a field its
/// kind needs is missing, or one its kind refuses is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum KindError {
    #[error("its kind {0:?} is none the release runs")]
    NotRun(ActionKind),
    #[error("its kind needs or refuses `{}`", .0.name())]
    Field(ActionDataField),
}

/// A kind's problem as the action's.
impl From<KindError> for ActionProblem {
    fn from(error: KindError) -> Self {
        match error {
            KindError::NotRun(kind) => Self::NotRun(kind),
            KindError::Field(field) => Self::KindField(field),
        }
    }
}
