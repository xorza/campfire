use std::error::Error;
use std::fmt;

use campfire_math::{PlayerSlot, Tick};

use crate::delegation::error::{DelegationError, ScopeError};

/// Why the log refused a packet of inputs. Packets come from the network, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// The packet holds no input.
    EmptyPacket,
    /// The slot is not in the session header.
    UnknownPlayer,
    /// A payload is longer than the header's max payload length.
    PayloadTooLarge,
    /// The packet holds more inputs than the header's max inputs per tick, or takes the inputs of
    /// one stamp past it.
    TooManyInputs,
    /// An input is stamped before the input before it in the player's chain.
    StampBack,
    /// The player's inputs in all pass what its ticks so far hold: the max inputs per tick for
    /// each tick up to the max input lead past the next.
    AheadOfTime,
    /// The log's positions, which fit a `u32`, do not reach past the packet.
    LogFull,
    /// The player's session key did not sign the chain head after the packet: the signature is
    /// forged, or an input is missing, out of order or altered.
    BadSignature,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::EmptyPacket => f.write_str("packet holds no input"),
            InputError::UnknownPlayer => f.write_str("input from a player not in the session"),
            InputError::PayloadTooLarge => f.write_str("input payload above the max length"),
            InputError::TooManyInputs => f.write_str("inputs above the max inputs per tick"),
            InputError::StampBack => f.write_str("input stamped before the input before it"),
            InputError::AheadOfTime => {
                f.write_str("more inputs than the ticks up to the max input lead hold")
            }
            InputError::LogFull => f.write_str("session log full"),
            InputError::BadSignature => {
                f.write_str("chain head not signed by the player's session key")
            }
        }
    }
}

impl Error for InputError {}

/// Why a log gives no segment seed. A published log is untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedError {
    /// The server seed is not revealed yet: the segment is not published.
    NotRevealed,
    /// The server seed is not the segment's seed of the chain the header commits to.
    WrongSeed,
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SeedError::NotRevealed => "the server seed is not revealed",
            SeedError::WrongSeed => {
                "the server seed is not the segment's seed of its committed chain"
            }
        })
    }
}

impl Error for SeedError {}

/// Why a header does not start a log. A published header is untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    /// More players than a `u32` slot counts.
    TooManyPlayers,
    /// The delegation of the player in `slot` is not a delegation.
    Delegation {
        slot: PlayerSlot,
        error: DelegationError,
    },
    /// The delegation of the player in `slot` grants another session.
    Scope { slot: PlayerSlot, error: ScopeError },
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::TooManyPlayers => f.write_str("more players than slots"),
            HeaderError::Delegation { slot, error } => {
                write!(f, "player {}: {error}", slot.get())
            }
            HeaderError::Scope { slot, error } => write!(f, "player {}: {error}", slot.get()),
        }
    }
}

impl Error for HeaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HeaderError::TooManyPlayers => None,
            HeaderError::Delegation { error, .. } => Some(error),
            HeaderError::Scope { error, .. } => Some(error),
        }
    }
}

/// Why bytes do not decode to a session log. A log file is untrusted, so every flaw is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogError {
    /// The format tag is missing.
    NotLog,
    /// The bytes end inside a field.
    Truncated,
    /// A value does not decode.
    Malformed(postcard::Error),
    /// The header does not start a log.
    Header(HeaderError),
    /// The log refuses a packet logged before `tick`, as it refuses one from the network.
    Input { tick: Tick, error: InputError },
    /// The revealed server seed is not the first segment's seed of the chain the header commits
    /// to.
    WrongSeed,
    /// Bytes remain after the reveal.
    Trailing,
    /// The bytes decode, but not from the one encoding the log has, such as an overlong varint.
    NotCanonical,
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::NotLog => f.write_str("not a session log"),
            LogError::Truncated => f.write_str("session log ends inside a field"),
            LogError::Malformed(_) => f.write_str("session log value does not decode"),
            LogError::Header(error) => write!(f, "session log header refused: {error}"),
            LogError::Input { tick, error } => {
                write!(f, "session log input before tick {tick} refused: {error}")
            }
            LogError::WrongSeed => {
                f.write_str("session log reveals a server seed that is not its first segment's")
            }
            LogError::Trailing => f.write_str("session log has trailing bytes"),
            LogError::NotCanonical => f.write_str("session log is not in its canonical encoding"),
        }
    }
}

impl Error for LogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LogError::Malformed(error) => Some(error),
            LogError::Header(error) => Some(error),
            LogError::Input { error, .. } => Some(error),
            _ => None,
        }
    }
}
