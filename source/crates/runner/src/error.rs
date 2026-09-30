use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use campfire_protocol::SeedError;

/// Why a session log does not start a match. A published log is untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    /// The log gives no segment seed.
    Seed(SeedError),
    /// The mode does not run at the tick rate the session's terms fix.
    TickRate(NonZeroU32),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Seed(error) => write!(f, "{error}"),
            StartError::TickRate(hz) => write!(f, "the mode does not run at {hz} ticks a second"),
        }
    }
}

impl Error for StartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StartError::Seed(error) => Some(error),
            StartError::TickRate(_) => None,
        }
    }
}
