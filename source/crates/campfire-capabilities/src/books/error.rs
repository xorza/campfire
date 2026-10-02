use std::error::Error;
use std::fmt;

use crate::actions::error::ActionError;
use crate::books::type_place::TypePlace;
use crate::mode::error::{ModeError, UnitKitError};
use crate::orders::error::AiError;
use crate::stats::error::ModifierProblem;
use crate::values::declared_name::DeclaredName;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookError {
    /// The unit type makes no unit.
    Kit {
        package: u16,
        unit_type: TypePlace,
        error: UnitKitError,
    },
    /// The unit type's AI does not load.
    Ai {
        package: u16,
        unit_type: TypePlace,
        error: AiError,
    },
    /// A time of the action does not count in ticks.
    Action {
        package: u16,
        action: DeclaredName,
        error: ActionError,
    },
    /// The modifier does not load.
    Modifier {
        package: u16,
        modifier: DeclaredName,
        problem: ModifierProblem,
    },
    /// A time of the area type does not count in ticks.
    AreaTime {
        package: u16,
        unit_type: DeclaredName,
    },
    /// The mode's teams, relations or map name what the mode does not have.
    Mode(ModeError),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::Kit {
                unit_type, error, ..
            } => write!(f, "{unit_type}: {error}"),
            BookError::Ai {
                unit_type, error, ..
            } => write!(f, "{unit_type}: {error}"),
            BookError::Action { action, error, .. } => write!(f, "action {action}: {error}"),
            BookError::Modifier {
                modifier, problem, ..
            } => write!(f, "modifier {modifier}: {problem}"),
            BookError::Mode(error) => write!(f, "{error}"),
            BookError::AreaTime { unit_type, .. } => {
                write!(
                    f,
                    "area type {unit_type}: a time too large to count in ticks"
                )
            }
        }
    }
}

impl Error for BookError {}
