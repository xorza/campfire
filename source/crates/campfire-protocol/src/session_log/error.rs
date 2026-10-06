use std::error::Error;
use std::fmt;

use campfire_common::{PlayerSlot, Tick};

use crate::delegation::error::{DelegationError, ScopeError};
use crate::server_input::error::ServerInputDecodeError;

/// Why the log refused a packet of inputs. Packets come from the network, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// The packet holds no input.
    EmptyPacket,
    /// The slot is not in the session header.
    UnknownPlayer,
    /// No player controls the slot now: a bot plays it, it is open, or its player left.
    NotPlayer,
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
            InputError::NotPlayer => f.write_str("input for a slot no player controls"),
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

/// Why the log refused a server input. The server signs only what it means to log, so each is a
/// fault of the server, or of a log someone altered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerInputError {
    /// The slot is not in the session header.
    UnknownSlot,
    /// The server key did not sign the input at its place.
    BadSignature,
    /// A bot's payload is longer than the header's max payload length.
    PayloadTooLarge,
    /// A bot's inputs pass the header's max inputs per tick in one tick.
    TooManyInputs,
    /// Bot commands for a slot no bot plays.
    NotBot,
    /// A join of a slot a player controls.
    Occupied,
    /// A join of a slot reserved for the player who left it, by another player.
    Reserved,
    /// A renewal, a leave or a link's change for a slot no player controls.
    NotPlayer,
    /// A renewal by another main key than the slot's player's.
    OtherPlayer,
    /// The delegation of a join or a renewal grants another session.
    Scope(ScopeError),
    /// The log's positions, which fit a `u32`, do not reach past the input.
    LogFull,
}

impl fmt::Display for ServerInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerInputError::UnknownSlot => {
                f.write_str("server input for a slot not in the session")
            }
            ServerInputError::BadSignature => {
                f.write_str("server input not signed by the server key at its place")
            }
            ServerInputError::PayloadTooLarge => f.write_str("bot payload above the max length"),
            ServerInputError::TooManyInputs => {
                f.write_str("bot inputs above the max inputs per tick")
            }
            ServerInputError::NotBot => f.write_str("bot commands for a slot no bot plays"),
            ServerInputError::Occupied => f.write_str("join of a slot a player controls"),
            ServerInputError::Reserved => {
                f.write_str("join of a slot reserved for the player who left it")
            }
            ServerInputError::NotPlayer => {
                f.write_str("server input for a slot no player controls")
            }
            ServerInputError::OtherPlayer => f.write_str("renewal by another player"),
            ServerInputError::Scope(error) => write!(f, "delegation refused: {error}"),
            ServerInputError::LogFull => f.write_str("session log full"),
        }
    }
}

impl Error for ServerInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ServerInputError::Scope(error) => Some(error),
            _ => None,
        }
    }
}

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
    /// More slots than a `u32` counts.
    TooManySlots,
    /// The delegation of the player in `slot` is not a delegation.
    Delegation {
        slot: PlayerSlot,
        error: DelegationError,
    },
    /// The delegation of the player in `slot` grants another session.
    Scope { slot: PlayerSlot, error: ScopeError },
    /// The header starts another count of slots than the terms plan.
    SlotCount,
    /// The slot `slot` starts otherwise than the terms plan it: a player, a bot or open.
    PlanMismatch { slot: PlayerSlot },
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::TooManySlots => f.write_str("more slots than a u32 counts"),
            HeaderError::Delegation { slot, error } => {
                write!(f, "player {}: {error}", slot.get())
            }
            HeaderError::Scope { slot, error } => write!(f, "player {}: {error}", slot.get()),
            HeaderError::SlotCount => f.write_str("slot starts other than the terms' slots"),
            HeaderError::PlanMismatch { slot } => {
                write!(
                    f,
                    "slot {} starts otherwise than the terms plan",
                    slot.get()
                )
            }
        }
    }
}

impl Error for HeaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HeaderError::TooManySlots
            | HeaderError::SlotCount
            | HeaderError::PlanMismatch { .. } => None,
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
    /// The log refuses a server input logged before `tick`.
    Server { tick: Tick, error: ServerInputError },
    /// A server input logged before `tick` does not decode, or its delegation does not parse.
    ServerDecode {
        tick: Tick,
        error: ServerInputDecodeError,
    },
    /// An entry is neither a player's packet nor a server input, or a slot's start is none of
    /// a player's, a bot's or an open one.
    UnknownEntry,
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
            LogError::Server { tick, error } => {
                write!(
                    f,
                    "session log server input before tick {tick} refused: {error}"
                )
            }
            LogError::ServerDecode { tick, error } => {
                write!(
                    f,
                    "session log server input before tick {tick} does not read: {error}"
                )
            }
            LogError::UnknownEntry => f.write_str("session log entry of no known kind"),
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
            LogError::Server { error, .. } => Some(error),
            LogError::ServerDecode { error, .. } => Some(error),
            _ => None,
        }
    }
}
