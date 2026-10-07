use campfire_capabilities::PlannedEffect;
use thiserror::Error;

/// What is wrong with an effect of an action's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EffectProblem {
    /// An effect the release does not run yet.
    #[error("`{}` is an effect the release does not run yet", .0.name())]
    Planned(PlannedEffect),
    /// A list of a delivery's hit or end on an action that delivers nothing, which never runs.
    #[error("the action delivers nothing to hit or end")]
    NoDelivery,
    /// An effect to the unit reached in a list that reaches none: `on_end`, or `on_resolve` of
    /// an action that aims at no unit.
    #[error("an effect to the unit reached, where the list reaches none")]
    NoUnit,
    /// A number below zero, at some rank.
    #[error("a number below zero")]
    Negative,
    /// A spawn in an avatar's or a loadout's action, whose package holds no unit type to spawn
    /// until summons come.
    #[error("spawns a unit from an avatar's or a loadout's action, which waits for summons")]
    Summon,
    /// A spawn of a unit type with a box body, which only a placement places, as an effect has
    /// no placement's checks.
    #[error("spawns a unit type with a box body, which only a placement places")]
    SpawnBox,
    /// A number past what a sim number holds, at some rank.
    #[error("a number past a sim number")]
    Overflow,
    /// A modifier's duration that is not a whole number of milliseconds within a `u32` at each
    /// rank, as a scaling param is not.
    #[error("a duration that is no whole number of milliseconds within a u32 at each rank")]
    Duration,
}
