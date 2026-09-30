use std::error::Error;
use std::fmt;

use campfire_script::ScriptError;

/// Why a unit type's AI does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone)]
pub enum AiError {
    /// The think period is too long to count in ticks.
    TimeTooLarge,
    /// The script does not compile.
    Script(ScriptError),
    /// The script has no `think(ctx, unit)`.
    NoThink,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::TimeTooLarge => f.write_str("think period too long to count in ticks"),
            AiError::Script(error) => write!(f, "{error}"),
            AiError::NoThink => f.write_str("AI script has no think(ctx, unit)"),
        }
    }
}

impl Error for AiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AiError::Script(error) => Some(error),
            AiError::TimeTooLarge | AiError::NoThink => None,
        }
    }
}
