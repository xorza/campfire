use std::num::{NonZeroU64, ParseIntError};

use campfire_common::DeclaredName;
use thiserror::Error;

/// Why a probe's text does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProbeSpecError {
    /// Not `<period_ms>:<units>`.
    #[error("a probe reads `<period_ms>:<units>`")]
    Shape,
    /// The period is no whole count of milliseconds above 0.
    #[error("a probe's period is no whole count of milliseconds above 0")]
    Period(#[source] ParseIntError),
    /// The units are none of `all`, `player=<n>`, `team=<n>`, `type=<name>`, `ids=<id>,<id>`
    /// and `state=<state>`.
    #[error(
        "a probe's units are `all`, `player=<n>`, `team=<n>`, `type=<name>`, `ids=<id>,<id>` or \
         `state=<state>`"
    )]
    Units,
    #[error("a probe's number does not read")]
    Number(#[source] ParseIntError),
    /// A player is counted from 1.
    #[error("a probe's player is counted from 1")]
    Player,
    /// A type's name is no name a package declares.
    #[error("a probe's type is no name a package declares")]
    Name,
    /// A state is none of `idle`, `walking`, `stuck`, `working`, `fighting` and `acting`.
    #[error("a probe's state is `idle`, `walking`, `stuck`, `working`, `fighting` or `acting`")]
    State,
}

/// Why a probe does not probe a match.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProbeRefusal {
    /// Its period is no whole count of the match's ticks, which a probe would round.
    #[error("the probe's period of {0} ms is no whole count of ticks")]
    Period(NonZeroU64),
    /// It names a unit type the match has not.
    #[error("the probe names no unit type {0}")]
    NoType(DeclaredName),
}
