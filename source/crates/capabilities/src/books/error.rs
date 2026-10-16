use campfire_actions::{ActionError, ActionProblem};
use campfire_items::ItemError;
use campfire_mode::{ModeError, ModeNamesError};
use campfire_orders::AiError;
use campfire_progression::TrackError;
use campfire_script::UnknownName;
use campfire_stats::ModifierProblem;
use campfire_units::TypesError;
use campfire_values::{DeclaredName, PackageIndex, PackagePath, Stat};
use thiserror::Error;

use crate::books::book_place::BookPlace;
use crate::books::type_place::TypePlace;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BookError {
    /// The unit type makes no unit.
    #[error("{unit_type}")]
    Start {
        package: PackageIndex,
        unit_type: TypePlace,
        #[source]
        error: UnitStartError,
    },
    /// The unit type walks and has a box body, which only a type that stands has.
    #[error("{unit_type}: a box body on a type that walks")]
    BoxWalks {
        package: PackageIndex,
        unit_type: TypePlace,
    },
    /// The unit type's AI does not load.
    #[error("{unit_type}")]
    Ai {
        package: PackageIndex,
        unit_type: TypePlace,
        #[source]
        error: AiError,
    },
    /// The action's data breaks a rule.
    #[error("action {action}")]
    Action {
        package: PackageIndex,
        action: DeclaredName,
        #[source]
        problem: ActionProblem,
    },
    /// The modifier does not load.
    #[error("modifier {modifier}")]
    Modifier {
        package: PackageIndex,
        modifier: DeclaredName,
        #[source]
        problem: ModifierProblem,
    },
    /// The item type's costs break a rule of the item book.
    #[error("item {item}")]
    Item {
        package: PackageIndex,
        item: DeclaredName,
        #[source]
        error: ItemError,
    },
    /// A time of the area type does not count in ticks.
    #[error("area type {unit_type}: a time too large to count in ticks")]
    AreaTime {
        package: PackageIndex,
        unit_type: DeclaredName,
    },
    /// The mode's teams, relations or map name what the mode does not have.
    #[error(transparent)]
    Mode(#[from] ModeError),
    /// A list of the mode's names does not load.
    #[error(transparent)]
    Names(#[from] ModeNamesError),
    /// The mode's tracks do not load.
    #[error(transparent)]
    Tracks(#[from] TrackError),
    /// The match's unit types or tags do not load.
    #[error(transparent)]
    Types(#[from] TypesError),
    /// Data names a script at `path` that the package does not hold.
    #[error("no script {path}")]
    Script {
        package: PackageIndex,
        path: PackagePath,
    },
    /// Data at `at` names what the mode or its packages do not have.
    #[error("{at}")]
    Unknown {
        package: PackageIndex,
        at: BookPlace,
        #[source]
        name: UnknownName,
    },
}

impl BookError {
    /// The error of `name`, which data at `at` of the package at `package` gives.
    pub(crate) const fn unknown(package: PackageIndex, at: BookPlace, name: UnknownName) -> Self {
        Self::Unknown { package, at, name }
    }

    /// The error of `error`, which the data of `action` of the package at `package` gives.
    pub(crate) fn of_action(
        package: PackageIndex,
        action: &DeclaredName,
        error: ActionError,
    ) -> Self {
        match error {
            ActionError::Problem(problem) => Self::Action {
                package,
                action: action.clone(),
                problem,
            },
            ActionError::Unknown(name) => {
                Self::unknown(package, BookPlace::Action(action.clone()), name)
            }
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
