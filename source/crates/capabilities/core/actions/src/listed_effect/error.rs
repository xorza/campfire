use campfire_common::DeclaredName;
use campfire_script::UnknownName;
use thiserror::Error;

use crate::list_hook::ListHook;
use crate::planned_effect::PlannedEffect;

/// A rule a listed effect breaks: a rule of its numbers, or the release's.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectRule {
    /// An effect the release does not run yet.
    #[error("`{}` is an effect the release does not run yet", .0.name())]
    Planned(PlannedEffect),
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
    /// A list of a delivery's hit or end on an action that delivers nothing, which never runs.
    #[error("the action delivers nothing to hit or end")]
    NoDelivery,
    /// An effect to the unit reached in a list that reaches none: `on_end`, or `on_resolve` of an
    /// action that aims at no unit.
    #[error("an effect to the unit reached, where the list reaches none")]
    NoUnit,
    /// A spawn of a unit type with a box body, which only a placement places, as an effect has no
    /// placement's checks.
    #[error("spawns a unit type with a box body, which only a placement places")]
    SpawnBox,
    /// A purge of a tag only the engine gives.
    #[error("purges {0}, a tag only the engine gives")]
    EngineTag(DeclaredName),
}

/// Why a listed effect does not resolve: a rule an effect of the list `list` breaks, the
/// effect's own list or one it holds, or a name the match does not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectError {
    #[error("its {list:?} list: {rule}")]
    Rule { list: ListHook, rule: EffectRule },
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}
