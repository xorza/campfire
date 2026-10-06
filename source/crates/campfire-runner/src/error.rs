use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use campfire_capabilities::CallError;
use campfire_common::StateHash;
use campfire_package::StoreError;
use campfire_protocol::{AfterLeave, Outcome, SeedError, ServerInputError, SlotChange};

/// Why a session log does not start a match. A published log is untrusted, and so are packages,
/// so each is an expected failure.
#[derive(Debug)]
pub enum StartError {
    /// The log gives no segment seed.
    Seed(SeedError),
    Terms(TermsError),
    /// The store does not give the packages the terms name.
    Packages(StoreError),
    /// A change of a slot's controller the mode's `[players]` does not allow.
    SlotRule {
        change: SlotChange,
        error: SlotRuleError,
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
            StartError::SlotRule { change, error } => write!(
                f,
                "slot {} in tick {}: {error}",
                change.slot.get(),
                change.tick
            ),
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
            StartError::SlotRule { error, .. } => Some(error),
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
    /// The terms plan `slots` slots, none or more than the `most` the mode's teams have.
    Slots { slots: u64, most: u64 },
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
            TermsError::Slots { slots, most } => {
                write!(f, "{slots} slots, and the mode's teams have {most}")
            }
        }
    }
}

impl Error for TermsError {}

/// Why the mode's `[players]` refuses a change of a slot's controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRuleError {
    /// A new player took an open slot, and the mode has no late join.
    LateJoin,
    /// A new player took a bot's slot, and the mode has no bot takeover.
    BotTakeover,
    /// A left player's slot became `becomes`, and the mode's `leaver` says otherwise.
    Leaver { becomes: AfterLeave },
}

impl fmt::Display for SlotRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlotRuleError::LateJoin => f.write_str("a late join, which the mode does not allow"),
            SlotRuleError::BotTakeover => {
                f.write_str("a bot's slot taken over, which the mode does not allow")
            }
            SlotRuleError::Leaver { becomes } => {
                write!(
                    f,
                    "a leaver's slot became {becomes:?}, which the mode does not say"
                )
            }
        }
    }
}

impl Error for SlotRuleError {}

/// Why a session refused a server input: the log's structure, or the mode's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerInputRefused {
    Log(ServerInputError),
    Rule(SlotRuleError),
}

impl fmt::Display for ServerInputRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerInputRefused::Log(error) => write!(f, "{error}"),
            ServerInputRefused::Rule(error) => write!(f, "{error}"),
        }
    }
}

impl Error for ServerInputRefused {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ServerInputRefused::Log(error) => Some(error),
            ServerInputRefused::Rule(error) => Some(error),
        }
    }
}

/// Why a log's result does not hold for the state its replay ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultMismatch {
    /// The result's state hash is not the replay's.
    Hash {
        logged: StateHash,
        replayed: StateHash,
    },
    /// The result is not the outcome the mode ended the match with.
    Outcome { logged: Outcome, ended: Outcome },
}

impl fmt::Display for ResultMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResultMismatch::Hash { logged, replayed } => write!(
                f,
                "the result's state hash {logged} is not the replay's {replayed}"
            ),
            ResultMismatch::Outcome { logged, ended } => write!(
                f,
                "the result says {logged:?}, and the match ended as {ended:?}"
            ),
        }
    }
}

impl Error for ResultMismatch {}
