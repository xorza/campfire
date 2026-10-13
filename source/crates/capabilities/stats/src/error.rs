use campfire_core::UnknownName;
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
