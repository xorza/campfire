use thiserror::Error;

use crate::actions::error::ActionError;
use crate::books::type_place::TypePlace;
use crate::mode::error::{ModeError, UnitKitError};
use crate::orders::error::AiError;
use crate::stats::error::ModifierProblem;
use crate::values::declared_name::DeclaredName;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BookError {
    /// The unit type makes no unit.
    #[error("{unit_type}: {error}")]
    Kit {
        package: u16,
        unit_type: TypePlace,
        #[source]
        error: UnitKitError,
    },
    /// The unit type's AI does not load.
    #[error("{unit_type}: {error}")]
    Ai {
        package: u16,
        unit_type: TypePlace,
        #[source]
        error: AiError,
    },
    /// A time of the action does not count in ticks.
    #[error("action {action}: {error}")]
    Action {
        package: u16,
        action: DeclaredName,
        #[source]
        error: ActionError,
    },
    /// The modifier does not load.
    #[error("modifier {modifier}: {problem}")]
    Modifier {
        package: u16,
        modifier: DeclaredName,
        #[source]
        problem: ModifierProblem,
    },
    /// A time of the area type does not count in ticks.
    #[error("area type {unit_type}: a time too large to count in ticks")]
    AreaTime {
        package: u16,
        unit_type: DeclaredName,
    },
    /// The mode's teams, relations or map name what the mode does not have.
    #[error("{0}")]
    Mode(#[source] ModeError),
}
