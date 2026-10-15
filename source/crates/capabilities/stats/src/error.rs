use campfire_script::{CapabilityRefusal, ParamProblem, UnknownName};
use campfire_sim::Capability;
use campfire_values::DeclaredName;
use thiserror::Error;

/// Why a modifier does not load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ModifierProblem {
    /// A value is past what a number holds.
    #[error("a value past what a number holds")]
    Overflow,
    /// A time is negative, or too large to count in ticks.
    #[error("a time negative or too large to count in ticks")]
    Time,
    /// An aura's radius or a shield is negative, as a value or at a rank of a param.
    #[error("an aura radius or a shield below zero")]
    Negative,
}

/// Why a modifier's spec does not build: a problem of its values, or a name the match does not
/// have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpecError {
    #[error(transparent)]
    Problem(#[from] ModifierProblem),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}

/// Why an edge does not join a stat graph: the stat a change reads, or the stat it changes, is
/// not one of the graph's.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EdgeError {
    #[error("the read stat: {0}")]
    Read(UnknownName),
    #[error("the changed stat: {0}")]
    Changed(UnknownName),
}

/// The modifier `modifier` of a package does not load, for `error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierError {
    pub modifier: DeclaredName,
    pub error: SpecError,
}

campfire_common::enum_all! {
    /// Why the stats capability refused a script call: a script gave it a value it does not take.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StatsRefusal {
        /// A modifier's param that the way it applies gives none of: an action that does not
        /// declare it, or no action at all.
        ParamMissing,
        /// A modifier's per-rank param with no value at the rank the way applies it at.
        ParamShort,
        /// A modifier's param whose value, at a rank, is past what a number holds.
        ParamOverflow,
        /// A modifier's time that reads a scaling param, whose value only its source knows as it
        /// applies.
        ParamScalingTime,
        /// A modifier's time that is, at a rank, negative or too large to count in ticks.
        ParamTime,
        /// A negative count of a modifier's stacks.
        NegativeStacks,
        /// A pool the unit does not have.
        NoPool,
        /// A stat or level field of a unit that has no stats.
        NoStats,
    }
    const ALL;
}

impl CapabilityRefusal for StatsRefusal {
    const OWNER: Capability = Capability::Stats;
    const CASES: &'static [StatsRefusal] = &StatsRefusal::ALL;

    fn message(self) -> &'static str {
        match self {
            StatsRefusal::ParamMissing => {
                "a param of the modifier, as the call applies it, is not given"
            }
            StatsRefusal::ParamShort => {
                "a param of the modifier has no value at the rank the call applies it at"
            }
            StatsRefusal::ParamOverflow => "a param of the modifier is past what a number holds",
            StatsRefusal::ParamScalingTime => "a time of the modifier reads a scaling param",
            StatsRefusal::ParamTime => {
                "a time of the modifier is negative or too large to count in ticks"
            }
            StatsRefusal::NegativeStacks => "a count of a modifier's stacks is negative",
            StatsRefusal::NoPool => "unit has no such pool",
            StatsRefusal::NoStats => "unit has no stats",
        }
    }
}

impl StatsRefusal {
    /// The refusal of a modifier's param that does not hold for `problem`.
    pub const fn param(problem: ParamProblem) -> StatsRefusal {
        match problem {
            ParamProblem::Missing => StatsRefusal::ParamMissing,
            ParamProblem::Short => StatsRefusal::ParamShort,
            ParamProblem::Overflow => StatsRefusal::ParamOverflow,
            ParamProblem::ScalingTime => StatsRefusal::ParamScalingTime,
            ParamProblem::Time => StatsRefusal::ParamTime,
        }
    }
}
