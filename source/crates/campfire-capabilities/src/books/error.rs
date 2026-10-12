use thiserror::Error;

use crate::books::book_place::BookPlace;
use crate::books::type_place::TypePlace;
use crate::mode::error::ModeError;
use crate::orders::error::AiError;
use crate::scripts::error::UnknownName;
use crate::stats::error::ModifierProblem;
use crate::values::declared_name::DeclaredName;
use crate::values::error::TimeTooLarge;
use crate::values::stat::Stat;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BookError {
    /// The unit type makes no unit.
    #[error("{unit_type}")]
    Start {
        package: u16,
        unit_type: TypePlace,
        #[source]
        error: UnitStartError,
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
