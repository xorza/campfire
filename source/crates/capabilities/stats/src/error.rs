use campfire_common::{DeclaredName, EnumAll};
use campfire_script::{CapabilityRefusal, ParamProblem, UnknownName};
use campfire_sim::Capability;
use campfire_values::Stat;
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

impl EdgeError {
    /// The stat the graph does not hold.
    pub fn name(self) -> UnknownName {
        match self {
            Self::Read(name) | Self::Changed(name) => name,
        }
    }
}

/// Why a package's modifier breaks a rule of stats. Packages are untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModifiersDataError {
    /// It grants a tag only the engine gives.
    #[error("the tag {0}, which only the engine gives")]
    EngineTag(DeclaredName),
}

/// Why a mode's stats have no order the stats refresh computes them in.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StatGraphError {
    /// Live stat changes across the mode's modifiers read each other in a loop, through these
    /// stats.
    #[error("live stat changes read each other in a loop through {}", joined(.0))]
    Loop(Vec<Stat>),
}

/// The names of `stats`, in their order.
fn joined(stats: &[Stat]) -> String {
    let names: Vec<String> = stats.iter().map(Stat::to_string).collect();
    names.join(", ")
}

/// The modifier `modifier` of a package does not load, for `error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierError {
    pub modifier: DeclaredName,
    pub error: SpecError,
}

/// Why the stats capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
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
    /// A negative amount to restore.
    NegativeRestore,
    /// A stat or level field of a unit that has no stats.
    NoStats,
}

impl CapabilityRefusal for StatsRefusal {
    const OWNER: Capability = Capability::Stats;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::ParamMissing => "a param of the modifier, as the call applies it, is not given",
            Self::ParamShort => {
                "a param of the modifier has no value at the rank the call applies it at"
            }
            Self::ParamOverflow => "a param of the modifier is past what a number holds",
            Self::ParamScalingTime => "a time of the modifier reads a scaling param",
            Self::ParamTime => "a time of the modifier is negative or too large to count in ticks",
            Self::NegativeStacks => "a count of a modifier's stacks is negative",
            Self::NoPool => "unit has no such pool",
            Self::NegativeRestore => "amount to restore is negative",
            Self::NoStats => "unit has no stats",
        }
    }
}

impl StatsRefusal {
    /// The refusal of a modifier's param that does not hold for `problem`.
    pub const fn param(problem: ParamProblem) -> Self {
        match problem {
            ParamProblem::Missing => Self::ParamMissing,
            ParamProblem::Short => Self::ParamShort,
            ParamProblem::Overflow => Self::ParamOverflow,
            ParamProblem::ScalingTime => Self::ParamScalingTime,
            ParamProblem::Time => Self::ParamTime,
        }
    }
}

/// Why a unit type's values do not make a unit. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnitStartError {
    /// A stat its pools need is not one it gives a value.
    #[error("stat {0} is not declared")]
    MissingStat(Stat),
    /// A pool's maximum is not positive at level 1.
    #[error("stat {0} is not positive")]
    NotPositive(Stat),
    /// It has a `combat` section but not the life pool.
    #[error("a unit type with combat lacks the life pool")]
    NoLifePool,
    /// It has the life pool but no `combat` section.
    #[error("a unit type with the life pool lacks combat")]
    NoCombat,
}
