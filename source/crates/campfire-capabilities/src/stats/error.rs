use std::error::Error;
use std::fmt;

use crate::values::declared_name::DeclaredName;

/// Why a modifier does not load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierProblem {
    /// A value is past what a number holds.
    Overflow,
    /// A time is negative, or too large to count in ticks.
    Time,
    /// An aura's radius or a shield is negative, as a value or at a rank of a param.
    Negative,
}

/// The modifier `modifier` of a package does not load, for `problem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModifierError {
    pub(crate) modifier: DeclaredName,
    pub(crate) problem: ModifierProblem,
}

impl fmt::Display for ModifierProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModifierProblem::Overflow => f.write_str("a value past what a number holds"),
            ModifierProblem::Time => f.write_str("a time negative or too large to count in ticks"),
            ModifierProblem::Negative => f.write_str("an aura radius or a shield below zero"),
        }
    }
}

impl Error for ModifierProblem {}
