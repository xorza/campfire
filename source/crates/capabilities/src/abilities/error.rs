use std::error::Error;
use std::fmt;

use campfire_script::ScriptError;

use crate::abilities::ability_data::Targeting;

/// Why an ability's data does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone)]
pub enum AbilityError {
    /// A targeting the engine does not run yet: a point or a direction.
    UnsupportedTargeting(Targeting),
    /// The ability's per-rank arrays have different lengths.
    RankCounts,
    /// The data names a script, but no source came with it, or the other way round.
    ScriptMismatch,
    /// A time is too large to count in ticks.
    TimeTooLarge,
    /// The script does not compile.
    Script(ScriptError),
}

impl fmt::Display for AbilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbilityError::UnsupportedTargeting(targeting) => {
                write!(f, "targeting {targeting:?} is not supported yet")
            }
            AbilityError::RankCounts => f.write_str("per-rank arrays of different lengths"),
            AbilityError::ScriptMismatch => {
                f.write_str("script named without a source, or a source for no script")
            }
            AbilityError::TimeTooLarge => f.write_str("time too large to count in ticks"),
            AbilityError::Script(error) => write!(f, "{error}"),
        }
    }
}

impl Error for AbilityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AbilityError::Script(error) => Some(error),
            _ => None,
        }
    }
}

/// Why a started cast did not resolve. A failed cast changes nothing.
#[derive(Debug, Clone)]
pub enum CastError {
    /// A param overflows at the ability's rank.
    ParamOverflow,
    /// The script API refused a call.
    Api(ApiError),
    /// The script failed otherwise.
    Script(ScriptError),
}

/// Why the script API refused a call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiError {
    /// A filter that is not `enemies`, `allies` or `all`.
    UnknownFilter,
    NegativeRadius,
    /// A damage kind that is not `physical`, `magic` or `true`.
    UnknownDamageKind,
    NegativeDamage,
    /// An integer beyond a `Num`, which reaches 2³⁹.
    IntegerBeyondNum,
}

impl fmt::Display for CastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CastError::ParamOverflow => f.write_str("a param overflows at the ability's rank"),
            CastError::Api(error) => write!(f, "the script API refused a call: {error}"),
            CastError::Script(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CastError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CastError::Api(error) => Some(error),
            CastError::Script(error) => Some(error),
            CastError::ParamOverflow => None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ApiError::UnknownFilter => "filter is not enemies, allies or all",
            ApiError::NegativeRadius => "radius is negative",
            ApiError::UnknownDamageKind => "damage kind is not physical, magic or true",
            ApiError::NegativeDamage => "damage is negative",
            ApiError::IntegerBeyondNum => "integer is beyond a Num",
        })
    }
}

impl Error for ApiError {}
