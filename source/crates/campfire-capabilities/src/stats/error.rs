use thiserror::Error;

use crate::values::declared_name::DeclaredName;

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

/// The modifier `modifier` of a package does not load, for `problem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModifierError {
    pub(crate) modifier: DeclaredName,
    pub(crate) problem: ModifierProblem,
}
