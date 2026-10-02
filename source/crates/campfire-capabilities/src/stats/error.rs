use std::error::Error;
use std::fmt;

use crate::values::declared_name::DeclaredName;

/// Why a modifier that passed the package load does not load into a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierProblem {
    /// A value is past what a number holds.
    Overflow,
    /// A time is negative, or too large to count in ticks.
    Time,
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
        }
    }
}

impl Error for ModifierProblem {}
