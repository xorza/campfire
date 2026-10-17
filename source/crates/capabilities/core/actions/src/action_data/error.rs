use campfire_common::DeclaredName;
use thiserror::Error;

use crate::effect_data::PlannedEffect;

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
    /// A weapon's own damage: a number at least 0 that a sim number holds.
    Amount,
    /// A weapon's own period: whole milliseconds, not 0.
    Period,
    /// A weapon's clip: shots of 1 to 65,535, and a reload of whole milliseconds.
    Clip,
}

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
