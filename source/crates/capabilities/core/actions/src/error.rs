use campfire_common::DeclaredName;
use campfire_script::UnknownName;
use campfire_sim::Capability;
use campfire_values::TimeTooLarge;
use thiserror::Error;

use crate::action_data::error::{ActionField, EffectRule};
use crate::action_data_field::ActionDataField;
use crate::action_kind::{ActionKind, MeteredKind};
use crate::list_hook::ListHook;

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
    /// It clamps its aim to its range, and aims at other than a point.
    #[error("only a point aim clamps to the range")]
    ClampAims,
    /// It holds a modifier, and has no toggle or channel to hold it while.
    #[error("a hold needs a toggle or a channel")]
    HoldAlone,
    /// A splash, and no projectile delivery for it to go off where it ends.
    #[error("a splash goes off where a projectile of its delivery ends")]
    Splash,
    /// A scatter, and no projectile delivery for it to aim.
    #[error("a scatter aims a projectile of its delivery")]
    Scatter,
    /// A speed, which delays the hit of a weapon of no delivery, and a delivery.
    #[error("a speed delays the hit of a weapon of no delivery")]
    Speed,
    /// An attack whose delivery is an area, where a weapon's is a homing projectile or a curve.
    #[error("a weapon's delivery is a homing projectile or a curve")]
    Weapon,
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
