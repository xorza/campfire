use std::error::Error;
use std::fmt;

use campfire_script::ScriptError;

/// Why an ability's data does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone)]
pub enum AbilityError {
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
