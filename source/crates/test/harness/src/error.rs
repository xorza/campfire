use std::error::Error;
use std::fmt;
use std::num::{NonZeroU64, ParseIntError};

use campfire_common::DeclaredName;
use campfire_store::{DurableDirError, DurableError, PathError};

/// Why a probe's text does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeSpecError {
    /// Not `<period_ms>:<units>`.
    Shape,
    /// The period is no whole count of milliseconds above 0.
    Period(ParseIntError),
    /// The units are none of `all`, `player=<n>`, `team=<n>`, `type=<name>` and `ids=<id>,<id>`.
    Units,
    Number(ParseIntError),
    /// A player is counted from 1.
    Player,
    /// A type's name is no name a package declares.
    Name,
}

/// Why a probe does not probe a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeRefusal {
    /// Its period is no whole count of the match's ticks, which a probe would round.
    Period(NonZeroU64),
    /// It names a unit type the match has not.
    NoType(DeclaredName),
}

/// Why a probe's file does not write.
#[derive(Debug)]
pub enum ProbeWriteError {
    Dir(PathError<DurableDirError>),
    File(PathError<DurableError>),
}

impl fmt::Display for ProbeSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Shape => "a probe reads `<period_ms>:<units>`",
            Self::Period(_) => "a probe's period is no whole count of milliseconds above 0",
            Self::Units => {
                "a probe's units are `all`, `player=<n>`, `team=<n>`, `type=<name>` or \
                 `ids=<id>,<id>`"
            }
            Self::Number(_) => "a probe's number does not read",
            Self::Player => "a probe's player is counted from 1",
            Self::Name => "a probe's type is no name a package declares",
        })
    }
}

impl Error for ProbeSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Period(error) | Self::Number(error) => Some(error),
            Self::Shape | Self::Units | Self::Player | Self::Name => None,
        }
    }
}

impl fmt::Display for ProbeRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Period(ms) => write!(
                f,
                "the probe's period of {ms} ms is no whole count of ticks"
            ),
            Self::NoType(name) => write!(f, "the probe names no unit type {name}"),
        }
    }
}

impl Error for ProbeRefusal {}

impl fmt::Display for ProbeWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dir(error) => {
                let dir = error.path.display();
                write!(f, "the probe's directory {dir} does not make")
            }
            Self::File(error) => {
                let file = error.path.display();
                write!(f, "the probe's file {file} does not write")
            }
        }
    }
}

impl Error for ProbeWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Dir(error) => Some(&error.error),
            Self::File(error) => Some(&error.error),
        }
    }
}

impl From<PathError<DurableDirError>> for ProbeWriteError {
    fn from(error: PathError<DurableDirError>) -> Self {
        Self::Dir(error)
    }
}

impl From<PathError<DurableError>> for ProbeWriteError {
    fn from(error: PathError<DurableError>) -> Self {
        Self::File(error)
    }
}
