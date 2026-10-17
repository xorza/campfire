use thiserror::Error;

/// What is wrong with an effect of an action's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EffectProblem {
    /// A list of a delivery's hit or end on an action that delivers nothing, which never runs.
    #[error("the action delivers nothing to hit or end")]
    NoDelivery,
    /// An effect to the unit reached in a list that reaches none: `on_end`, or `on_resolve` of
    /// an action that aims at no unit.
    #[error("an effect to the unit reached, where the list reaches none")]
    NoUnit,
    /// A spawn of a unit type with a box body, which only a placement places, as an effect has
    /// no placement's checks.
    #[error("spawns a unit type with a box body, which only a placement places")]
    SpawnBox,
}
