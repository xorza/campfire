use std::fmt;

use campfire_capabilities::PlannedEffect;

/// What is wrong with an effect of an action's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectProblem {
    /// An effect the release does not run yet.
    Planned(PlannedEffect),
    /// A list of a delivery's hit or end on an action that delivers nothing, which never runs.
    NoDelivery,
    /// An effect to the unit reached in a list that reaches none: `on_end`, or `on_resolve` of
    /// an action that aims at no unit.
    NoUnit,
    /// A number below zero, at some rank.
    Negative,
    /// A spawn in an avatar's or a loadout's action, whose package holds no unit type to spawn
    /// until summons come.
    Summon,
    /// A number past what a sim number holds, at some rank.
    Overflow,
    /// A modifier's duration that is not a whole number of milliseconds within a `u32` at each
    /// rank, as a scaling param is not.
    Duration,
}

impl fmt::Display for EffectProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EffectProblem::Planned(effect) => {
                write!(
                    f,
                    "`{}` is an effect the release does not run yet",
                    effect.name()
                )
            }
            EffectProblem::NoDelivery => f.write_str("the action delivers nothing to hit or end"),
            EffectProblem::NoUnit => {
                f.write_str("an effect to the unit reached, where the list reaches none")
            }
            EffectProblem::Negative => f.write_str("a number below zero"),
            EffectProblem::Summon => f.write_str(
                "spawns a unit from an avatar's or a loadout's action, which waits for summons",
            ),
            EffectProblem::Overflow => f.write_str("a number past a sim number"),
            EffectProblem::Duration => f.write_str(
                "a duration that is no whole number of milliseconds within a u32 at each rank",
            ),
        }
    }
}
