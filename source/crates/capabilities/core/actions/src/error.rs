use campfire_common::DeclaredName;
use campfire_names::UnknownName;
use campfire_sim::Capability;
use campfire_values::TimeTooLarge;
use thiserror::Error;

use crate::list_hook::ListHook;
use crate::listed_effect::error::{EffectError, EffectRule};
use crate::pipeline_field::PipelineField;

/// Why an action's data makes no action of a match: a rule of the load's checks that its data
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
    /// A field of the pipeline its kind always takes is missing, or one its kind never takes is
    /// given.
    #[error("its kind needs or refuses `{}`", .0.name())]
    KindField(PipelineField),
    /// A range global at a rank, where its kind needs meters at every rank.
    #[error("its range is global at a rank, where its kind needs meters")]
    Global,
    /// A minimum range at a rank whose range is global or no farther.
    #[error("its minimum range lies within its range of meters")]
    MinRange,
    /// The projectile type its delivery names lacks its `projectile` section.
    #[error("unit type {0} lacks the projectile section its delivery needs")]
    Section(DeclaredName),
    /// A number or a duration of an effect of its list `list` breaks `rule`.
    #[error("its {list:?} list: {rule}")]
    Effect { list: ListHook, rule: EffectRule },
    /// It clamps its aim to its range, and aims at other than a point.
    #[error("only a point aim clamps to the range")]
    ClampAims,
    /// It holds a modifier, and has no toggle or channel to hold it while.
    #[error("a hold needs a toggle or a channel")]
    HoldAlone,
    /// An avatar's action in no slot of its package's unit types.
    #[error("in no slot of its package's unit types")]
    Unslotted,
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

/// Why a unit type's slots break a rule of the slot kinds a capability claims. Packages are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SlotError {
    /// A claimed kind of more than one rank, where an item's and a use's action have one.
    #[error("slot kind {0}, which it claims, has more than one rank")]
    RankedClaim(DeclaredName),
    /// A kind two capabilities claim.
    #[error("slot kind {0}, which another capability claims")]
    ClaimedTwice(DeclaredName),
    /// Actions in a kind whose slots `by` fills.
    #[error("actions in slot kind {kind}, whose slots {by:?} fills")]
    ClaimedSlotted { kind: DeclaredName, by: Capability },
    /// An action in two of its slots.
    #[error("action {0} in two slots")]
    Repeated(DeclaredName),
    /// More slots, its own and those its claims fill, than a unit holds.
    #[error("more slots than a unit holds")]
    TooMany,
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

impl From<EffectError> for ActionError {
    fn from(error: EffectError) -> Self {
        match error {
            EffectError::Rule { list, rule } => Self::Problem(ActionProblem::Effect { list, rule }),
            EffectError::Unknown(name) => Self::Unknown(name),
        }
    }
}

impl From<TimeTooLarge> for ActionError {
    fn from(error: TimeTooLarge) -> Self {
        Self::Problem(ActionProblem::Time(error))
    }
}

/// A field that does not hold at a rank is the action's problem.
impl From<ActionField> for ActionProblem {
    fn from(field: ActionField) -> Self {
        Self::Field(field)
    }
}

/// A capability field of an action that the release runs, which does not give a value of its
/// kind at a rank: a whole number of milliseconds or of a pool the mode declares, or a range of
/// meters that is not negative, and never through a scaling param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionField {
    Range,
    MinRange,
    Cooldown,
    Cost,
    Windup,
    /// Its charges: a count of 1 to 255, and a recharge of whole milliseconds.
    Charges,
    /// Its toggle's cost: whole amounts of the caster's pools.
    Toggle,
    /// Its channel: a length, a time between ticks and a wind-down of whole milliseconds, none
    /// of them 0, and a windup spread of 0 to below 1.
    Channel,
    /// Its charge's most: whole milliseconds, not 0.
    Charge,
}
