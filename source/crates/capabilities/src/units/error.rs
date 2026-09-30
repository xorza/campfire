use std::error::Error;
use std::fmt;

use campfire_math::Num;
use campfire_script::rhai::{EvalAltResult, INT};
use campfire_script::{Raised, ScriptError};

/// What a registered script function returns: its value, or the error that fails the call.
pub(crate) type Checked<T> = Result<T, Box<EvalAltResult>>;

/// Why a unit type does not load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitTypeError {
    /// The match's unit types would have more than 64 tags.
    TooManyTags,
    /// The match would have more unit types than a `u16` counts.
    TooManyTypes,
}

/// Why a script call failed. A failed call changes nothing.
#[derive(Debug, Clone)]
pub enum CallError {
    /// A param of the ability overflows at its rank.
    ParamOverflow,
    /// The script API refused a call.
    Api(ApiError),
    /// The script failed otherwise.
    Script(ScriptError),
}

/// Why the script API refused a call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiError {
    /// A param the ability or the unit type does not declare.
    UnknownParam,
    /// A filter whose relation is not `enemies`, `allies` or `all`.
    UnknownFilter,
    /// A filter's tag that no unit type of the match declares.
    UnknownTag,
    NegativeRadius,
    NegativeTime,
    /// A damage kind that is not `physical`, `magic` or `true`.
    UnknownDamageKind,
    NegativeDamage,
    /// An integer beyond a `Num`, which reaches 2³⁹.
    IntegerBeyondNum,
    /// An attack field or order for a unit that has no attack.
    NoAttack,
    /// An AI order for another unit than the one that thinks.
    OtherUnit,
    /// An attack order on a unit that is not a living enemy.
    NotAnEnemy,
}

impl CallError {
    /// A failed call's error: the API's own when it refused the call.
    pub(crate) fn from_script(error: ScriptError) -> CallError {
        let refused = match &error {
            ScriptError::Raised(raised) => raised.get::<ApiError>(),
            _ => None,
        };
        refused.map_or(CallError::Script(error), CallError::Api)
    }
}

impl ApiError {
    /// The error that fails the call with this refusal.
    pub(crate) fn fail(self) -> EvalAltResult {
        Raised::error(self)
    }

    /// A script's integer as a `Num`; one beyond a `Num` fails the call.
    pub(crate) fn num(value: INT) -> Checked<Num> {
        Num::from_int(value).ok_or_else(|| ApiError::IntegerBeyondNum.fail().into())
    }
}

impl fmt::Display for UnitTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UnitTypeError::TooManyTags => "more than 64 unit tags",
            UnitTypeError::TooManyTypes => "more unit types than a u16 counts",
        })
    }
}

impl Error for UnitTypeError {}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::ParamOverflow => f.write_str("a param overflows at the ability's rank"),
            CallError::Api(error) => write!(f, "the script API refused a call: {error}"),
            CallError::Script(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CallError::Api(error) => Some(error),
            CallError::Script(error) => Some(error),
            CallError::ParamOverflow => None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ApiError::UnknownParam => "param is not declared",
            ApiError::UnknownFilter => "filter is not enemies, allies or all",
            ApiError::UnknownTag => "tag is not declared by any unit type",
            ApiError::NegativeRadius => "radius is negative",
            ApiError::NegativeTime => "time is negative",
            ApiError::UnknownDamageKind => "damage kind is not physical, magic or true",
            ApiError::NegativeDamage => "damage is negative",
            ApiError::IntegerBeyondNum => "integer is beyond a Num",
            ApiError::NoAttack => "unit has no attack",
            ApiError::OtherUnit => "an AI orders only the unit that thinks",
            ApiError::NotAnEnemy => "target is not a living enemy",
        })
    }
}

impl Error for ApiError {}
