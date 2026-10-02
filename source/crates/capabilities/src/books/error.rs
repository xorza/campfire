use std::error::Error;
use std::fmt;

use crate::actions::error::ActionError;
use crate::mode::error::UnitKitError;
use crate::orders::error::AiError;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookError {
    /// The unit type, by its name in its scope, makes no unit.
    Kit {
        package: u16,
        unit_type: String,
        error: UnitKitError,
    },
    /// The unit type's AI does not load.
    Ai {
        package: u16,
        unit_type: String,
        error: AiError,
    },
    /// A time of the action does not count in ticks.
    Action {
        package: u16,
        action: String,
        error: ActionError,
    },
    /// A time of the area type does not count in ticks.
    AreaTime { package: u16, unit_type: String },
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::Kit {
                unit_type, error, ..
            } => write!(f, "unit type {unit_type}: {error}"),
            BookError::Ai {
                unit_type, error, ..
            } => write!(f, "unit type {unit_type}: {error}"),
            BookError::Action { action, error, .. } => write!(f, "action {action}: {error}"),
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
