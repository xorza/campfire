use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use campfire_capabilities::{ActionError, AiError, CallError, ModeError, UnitKitError};
use campfire_package::StoreError;
use campfire_protocol::SeedError;

/// Why a session log does not start a match. A published log is untrusted, and so are packages,
/// so each is an expected failure.
#[derive(Debug)]
pub enum StartError {
    /// The log gives no segment seed.
    Seed(SeedError),
    /// The tick rate the session's terms fix is outside the mode's range.
    TickRate(NonZeroU32),
    /// The terms name another engine release than this one.
    OtherRelease(String),
    /// The packages are another mode than the one the terms name.
    OtherMode,
    /// The packages' dependencies are others than the ones the terms name.
    OtherDependencies,
    /// The store does not give the packages the terms name.
    Packages(StoreError),
    /// A unit type's values do not make a unit.
    UnitKit {
        unit_type: String,
        error: UnitKitError,
    },
    /// A unit type's AI does not load.
    Ai { unit_type: String, error: AiError },
    /// An ability does not load.
    Ability { ability: String, error: ActionError },
    /// The mode's setup does not start a match.
    Mode(ModeError),
    /// The mode script's `on_match_start` failed.
    MatchStart(CallError),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Seed(error) => write!(f, "{error}"),
            StartError::TickRate(hz) => {
                write!(f, "{hz} ticks a second is outside the mode's range")
            }
            StartError::OtherRelease(release) => {
                write!(
                    f,
                    "the session runs on engine release {release:?}, not this one"
                )
            }
            StartError::OtherMode => f.write_str("the mode is not the one the session names"),
            StartError::OtherDependencies => {
                f.write_str("the dependencies are not the ones the session names")
            }
            StartError::Packages(error) => write!(f, "{error}"),
            StartError::UnitKit { unit_type, error } => write!(f, "unit type {unit_type}: {error}"),
            StartError::Ai { unit_type, error } => write!(f, "unit type {unit_type}: {error}"),
            StartError::Ability { ability, error } => write!(f, "ability {ability}: {error}"),
            StartError::Mode(error) => write!(f, "{error}"),
            StartError::MatchStart(error) => write!(f, "the mode's start failed: {error}"),
        }
    }
}

impl Error for StartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StartError::Seed(error) => Some(error),
            StartError::Packages(error) => Some(error),
            StartError::UnitKit { error, .. } => Some(error),
            StartError::Ai { error, .. } => Some(error),
            StartError::Ability { error, .. } => Some(error),
            StartError::Mode(error) => Some(error),
            StartError::MatchStart(error) => Some(error),
            _ => None,
        }
    }
}
