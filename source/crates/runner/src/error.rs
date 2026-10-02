use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use campfire_capabilities::CallError;
use campfire_package::StoreError;
use campfire_protocol::SeedError;

/// Why a session log does not start a match. A published log is untrusted, and so are packages,
/// so each is an expected failure.
#[derive(Debug)]
pub enum StartError {
    /// The log gives no segment seed.
    Seed(SeedError),
    Terms(TermsError),
    /// The store does not give the packages the terms name.
    Packages(StoreError),
    /// More players than the mode's teams have slots.
    Players {
        players: u32,
        slots: u64,
    },
    /// The mode script's `on_match_start` failed.
    MatchStart(CallError),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Seed(error) => write!(f, "{error}"),
            StartError::Terms(error) => write!(f, "{error}"),
            StartError::Packages(error) => write!(f, "{error}"),
            StartError::Players { players, slots } => {
                write!(
                    f,
                    "{players} players, and the mode's teams have {slots} slots"
                )
            }
            StartError::MatchStart(error) => write!(f, "the mode's start failed: {error}"),
        }
    }
}

impl Error for StartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StartError::Seed(error) => Some(error),
            StartError::Terms(error) => Some(error),
            StartError::Packages(error) => Some(error),
            StartError::MatchStart(error) => Some(error),
            StartError::Players { .. } => None,
        }
    }
}

/// Why session terms do not name a session of a mode's packages on this release. Terms come from
/// a server or a published log, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermsError {
    /// The terms name another engine release than this one.
    OtherRelease(String),
    /// The packages are another mode than the one the terms name.
    OtherMode,
    /// The packages' dependencies are others than the ones the terms name.
    OtherDependencies,
    /// The tick rate the terms fix is outside the mode's range.
    TickRate(NonZeroU32),
}

impl fmt::Display for TermsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TermsError::OtherRelease(release) => {
                write!(
                    f,
                    "the session runs on engine release {release:?}, not this one"
                )
            }
            TermsError::OtherMode => f.write_str("the mode is not the one the session names"),
            TermsError::OtherDependencies => {
                f.write_str("the dependencies are not the ones the session names")
            }
            TermsError::TickRate(hz) => {
                write!(f, "{hz} ticks a second is outside the mode's range")
            }
        }
    }
}

impl Error for TermsError {}
