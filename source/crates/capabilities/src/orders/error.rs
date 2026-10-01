use std::error::Error;
use std::fmt;

/// Why a unit type's AI does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiError {
    /// The think period is too long to count in ticks.
    TimeTooLarge,
    /// The script has no `on_think(ctx, unit)`.
    NoThink,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::TimeTooLarge => f.write_str("think period too long to count in ticks"),
            AiError::NoThink => f.write_str("AI script has no on_think(ctx, unit)"),
        }
    }
}

impl Error for AiError {}
