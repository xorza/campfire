use thiserror::Error;

use crate::books::book_place::BookPlace;
use crate::books::type_place::TypePlace;
use crate::mode::error::{ModeError, UnitKitError};
use crate::orders::error::AiError;
use crate::scripts::error::UnknownName;
use crate::stats::error::ModifierProblem;
use crate::values::declared_name::DeclaredName;
use crate::values::error::TimeTooLarge;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BookError {
    /// The unit type makes no unit.
    #[error("{unit_type}")]
    Kit {
        package: u16,
        unit_type: TypePlace,
        #[source]
        error: UnitKitError,
    },
    /// The unit type's AI does not load.
    #[error("{unit_type}")]
    Ai {
        package: u16,
        unit_type: TypePlace,
        #[source]
        error: AiError,
    },
    /// A time of the action does not count in ticks.
    #[error("action {action}")]
    Action {
        package: u16,
        action: DeclaredName,
        #[source]
        error: TimeTooLarge,
    },
    /// The modifier does not load.
    #[error("modifier {modifier}")]
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
    #[error(transparent)]
    Mode(ModeError),
    /// Data at `at` names what the mode or its packages do not have.
    #[error("{at}")]
    Unknown {
        package: u16,
        at: BookPlace,
        #[source]
        name: UnknownName,
    },
}

impl BookError {
    /// The error of `name`, which data at `at` of the package at `package` gives.
    pub(crate) const fn unknown(package: u16, at: BookPlace, name: UnknownName) -> BookError {
        BookError::Unknown { package, at, name }
    }
}
