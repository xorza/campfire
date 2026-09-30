use std::error::Error;
use std::fmt;

use campfire_script::ScriptError;

/// A capability field of an ability that the release runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityField {
    Range,
    Cooldown,
    Cost,
    CastTime,
}

/// Why an ability's data does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone)]
pub enum AbilityError {
    /// A per-rank array of the ability has another length than its ranks.
    RankCount(u8),
    /// A capability field does not give a value of its kind at a rank: a whole number of
    /// milliseconds or of the resource, or a range of meters that is not negative, and never
    /// through a scaling param.
    Field(AbilityField),
    /// A unit target's filter names a tag no unit type of the match declares.
    UnknownTag(String),
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
            AbilityError::RankCount(ranks) => {
                write!(f, "a per-rank array without {ranks} entries")
            }
            AbilityError::Field(field) => write!(f, "{field:?} gives no value of its kind"),
            AbilityError::UnknownTag(filter) => write!(f, "targeting {filter:?}: no such tag"),
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
