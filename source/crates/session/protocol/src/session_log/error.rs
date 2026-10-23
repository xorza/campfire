use std::io;

use campfire_codec::Malformed;
use campfire_common::{PlayerSlot, Tick, Version};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::checkpoint::error::CheckpointDecodeError;
use crate::delegation::error::{DelegationError, ScopeError};
use crate::server_input::error::ServerInputDecodeError;
use crate::stamp_count::error::StampError;

/// Why the log refused a packet of inputs. Packets come from the network, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InputError {
    /// The packet holds no input.
    #[error("packet holds no input")]
    EmptyPacket,
    /// The player slot is not in the session header.
    #[error("input from a player not in the session")]
    UnknownPlayer,
    /// No player controls the player slot now: a bot plays it, it is open, or its player left.
    #[error("input for a player slot no player controls")]
    NotPlayer,
    /// A payload is longer than the header's max payload length.
    #[error("input payload above the max length")]
    PayloadTooLarge,
    /// The packet holds more inputs than the header's max inputs per tick, or takes the inputs of
    /// one stamp past it.
    #[error("inputs above the max inputs per tick")]
    TooManyInputs,
    /// An input is stamped before the input before it in the player's chain.
    #[error("input stamped before the input before it")]
    StampBack,
    /// The player's inputs in all pass what its ticks so far hold: the max inputs per tick for
    /// each tick up to the max input lead past the next.
    #[error("more inputs than the ticks up to the max input lead hold")]
    AheadOfTime,
    /// The log's file or its journal has no room for the packet and the session's end after it.
    #[error("session log full")]
    LogFull,
    /// The player's session key did not sign the chain head after the packet: the signature is
    /// forged, or an input is missing, out of order or altered.
    #[error("chain head not signed by the player's session key")]
    BadSignature,
}

impl From<StampError> for InputError {
    fn from(error: StampError) -> Self {
        match error {
            StampError::Back => Self::StampBack,
            StampError::TooMany => Self::TooManyInputs,
        }
    }
}

/// Why the log refused a server input. The server signs only what it means to log, so each is a
/// fault of the server, or of a log someone altered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ServerInputError {
    /// The player slot is not in the session header.
    #[error("server input for a player slot not in the session")]
    UnknownPlayerSlot,
    /// The server key did not sign the input at its place.
    #[error("server input not signed by the server key at its place")]
    BadSignature,
    /// A bot's payload is longer than the header's max payload length.
    #[error("bot payload above the max length")]
    PayloadTooLarge,
    /// A bot's inputs fill every tick up to the max input lead past the next with the header's
    /// max inputs per tick.
    #[error("bot inputs fill the ticks up to the max input lead")]
    AheadOfTime,
    /// Bot commands for a player slot no bot plays.
    #[error("bot commands for a player slot no bot plays")]
    NotBot,
    /// A join of a player slot a player controls.
    #[error("join of a player slot a player controls")]
    Occupied,
    /// A join of a player slot reserved for the player who left it, by another player.
    #[error("join of a player slot reserved for the player who left it")]
    Reserved,
    /// A join by a main key whose player controls another player slot: one player slot for each
    /// main key.
    #[error("join by a player who controls another player slot")]
    MainKeyHeld,
    /// A renewal, a leave or a link's change for a player slot no player controls.
    #[error("server input for a player slot no player controls")]
    NotPlayer,
    /// A renewal by another main key than the player slot's player's.
    #[error("renewal by another player")]
    OtherPlayer,
    /// The delegation of a join or a renewal grants another session.
    #[error("delegation refused")]
    Scope(#[source] ScopeError),
    /// The log's file or its journal has no room for the input and the session's end after it.
    #[error("session log full")]
    LogFull,
}

/// Why the log refused a checkpoint record. The server signs only what it means to log, so each
/// is a fault of the server, or of a log someone altered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CheckpointError {
    /// The server key did not sign the record.
    #[error("checkpoint not signed by the server key")]
    BadSignature,
    /// The record starts another segment than the oldest checkpoint begun that has none, or a
    /// begin starts another than the next.
    #[error("checkpoint of another segment than the one begun")]
    Segment,
    /// The record starts its segment at another tick than its begin, or a begin at another than
    /// the next.
    #[error("checkpoint at another tick than the one begun")]
    Tick,
    /// The last segment holds no tick.
    #[error("checkpoint ends a segment of no tick")]
    Empty,
    /// The record carries another state than the log's own at the boundary.
    #[error("checkpoint carries another state than the log's")]
    Carry,
    /// The log began no checkpoint that the record would complete.
    #[error("no checkpoint begun for the record")]
    NotBegun,
    /// The record a log file starts at carries no state a log holds at a boundary.
    #[error("checkpoint carries no state a log holds: {0}")]
    BadCarry(CarryFlaw),
}

/// What makes a checkpoint's carry no state a log holds at its boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CarryFlaw {
    #[error("another count of player slots than the header's")]
    PlayerSlots,
    #[error("player slot {0:?}'s delegation is not of this session")]
    Scope(PlayerSlot),
    #[error("player slot {0:?}'s main key controls a player slot before it")]
    MainKeyHeld(PlayerSlot),
    #[error(
        "player slot {0:?}'s chain is another player slot's, or counts other inputs than its stamps"
    )]
    Chain(PlayerSlot),
    #[error("player slot {0:?}'s stamps count inputs no packet sends")]
    Stamps(PlayerSlot),
    #[error("player slot {0:?}'s spill holds more inputs than a tick takes")]
    Spill(PlayerSlot),
    #[error(
        "input {0} is of no player slot that takes one, before the boundary, out of order, past the max \
         inputs of its tick or the max payload length"
    )]
    Pending(usize),
}

/// Why the log refused a result. The server signs only what it means to log, so each is a fault
/// of the server, or of a log someone altered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ResultError {
    /// The server key did not sign the result.
    #[error("result not signed by the server key")]
    BadSignature,
    /// The result stops before another tick than the next.
    #[error("result at another tick than the next")]
    Tick,
}

/// Why a log gives no segment seed. A published log is untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SeedError {
    /// The server seed is not revealed yet: the segment is not published.
    #[error("the server seed is not revealed")]
    NotRevealed,
    /// The server seed is not the segment's seed of the chain the header commits to.
    #[error("the server seed is not the segment's seed of its committed chain")]
    WrongSeed,
}

/// Why a header does not start a log. A published header is untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HeaderError {
    /// More player slots than a `u32` counts.
    #[error("more player slots than a u32 counts")]
    TooManyPlayerSlots,
    /// The delegation of the player in `player_slot` is not a delegation.
    #[error("player {}", .player_slot.get())]
    Delegation {
        player_slot: PlayerSlot,
        #[source]
        error: DelegationError,
    },
    /// The delegation of the player in `player_slot` grants another session.
    #[error("player {}", .player_slot.get())]
    Scope {
        player_slot: PlayerSlot,
        #[source]
        error: ScopeError,
    },
    /// The header starts another count of player slots than the terms plan.
    #[error("player slot starts other than the terms' player slots")]
    PlayerSlotCount,
    /// The player slot `player_slot` starts otherwise than the terms plan it: a player, a bot or
    /// open.
    #[error("player slot {} starts otherwise than the terms plan", .player_slot.get())]
    PlanMismatch { player_slot: PlayerSlot },
    /// The player in `player_slot` holds the main key of a player in an earlier player slot: one
    /// player slot for each main key.
    #[error("player slot {} starts the main key of an earlier player slot's player", .player_slot.get())]
    MainKeyRepeated { player_slot: PlayerSlot },
    /// The file starts the player slot `player_slot` as no known kind of start.
    #[error("player slot {} starts as no known kind", .player_slot.get())]
    UnknownStart { player_slot: PlayerSlot },
    /// The terms let a journal record hold more than a journal frame takes: inputs, payloads, a
    /// window of stamps or player slots past what a record of 16 MiB holds.
    #[error("the terms let a journal record pass 16 MiB")]
    RecordTooLarge,
}

/// Why a log does not load a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LogLoadError {
    /// The log holds no checkpoint that starts the segment.
    #[error("no checkpoint starts the segment")]
    NoCheckpoint,
    /// The journal has no room for the load's record and the session's end after it.
    #[error("session log full")]
    LogFull,
}

/// Why bytes do not decode to a session log. A log file is untrusted, so every flaw is an error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LogError {
    /// The format tag is missing.
    #[error("not a session log")]
    NotLog,
    /// The bytes end inside a field.
    #[error("session log ends inside a field")]
    Truncated,
    /// A value does not decode.
    #[error("session log value does not decode")]
    Malformed(#[source] Malformed),
    /// The header does not start a log.
    #[error("session log header refused")]
    Header(#[source] HeaderError),
    /// The log refuses a packet logged before `tick`, as it refuses one from the network.
    #[error("session log input before tick {tick} refused")]
    Input {
        tick: Tick,
        #[source]
        error: InputError,
    },
    /// The log refuses a server input logged before `tick`.
    #[error("session log server input before tick {tick} refused")]
    Server {
        tick: Tick,
        #[source]
        error: ServerInputError,
    },
    /// A server input logged before `tick` does not decode, or its delegation does not parse.
    #[error("session log server input before tick {tick} does not read")]
    ServerDecode {
        tick: Tick,
        #[source]
        error: ServerInputDecodeError,
    },
    /// An entry is neither a player's packet nor a server input.
    #[error("session log entry of no known kind")]
    UnknownEntry,
    /// The file holds no segment.
    #[error("session log holds no segment")]
    NoSegment,
    /// The checkpoint record of segment `segment` does not decode, or a delegation it carries
    /// does not parse.
    #[error("session log checkpoint of segment {segment} does not read")]
    CheckpointDecode {
        segment: u32,
        #[source]
        error: CheckpointDecodeError,
    },
    /// The log refuses the checkpoint record of segment `segment`.
    #[error("session log checkpoint of segment {segment} refused")]
    Checkpoint {
        segment: u32,
        #[source]
        error: CheckpointError,
    },
    /// The revealed server seed is not the last segment's seed of the chain the header commits
    /// to.
    #[error("session log reveals a server seed that is not its last segment's")]
    WrongSeed,
    /// The log refuses the result.
    #[error("session log result refused")]
    Result(#[source] ResultError),
    /// The journal's load of the save that starts segment `segment` does not load.
    #[error("session log load of segment {segment} refused")]
    Load {
        segment: u32,
        #[source]
        error: LogLoadError,
    },
    /// Bytes remain after the result.
    #[error("session log has trailing bytes")]
    Trailing,
    /// The bytes decode, but not from the one encoding the log has, such as an overlong varint.
    #[error("session log is not in its canonical encoding")]
    NotCanonical,
}

/// Why a journal's records do not rebuild a log. The server writes only records its log took, so
/// each is a fault of the journal's bytes, past the checks of its frames.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JournalReplayError {
    /// The journal holds no record, or its first is not the header.
    #[error("the journal starts with no header")]
    NoHeader,
    /// The header names the session's release, another than the one that reads it.
    #[error("the session runs on release {0}, which reads its journal")]
    OtherRelease(Version),
    /// The record of index `record`, from 0, comes after the result, which ends the session.
    #[error("journal record {record} comes after the session's result")]
    AfterResult { record: u64 },
    /// The record of index `record`, from 0, begins a checkpoint after an entry of the tick it
    /// comes before.
    #[error("journal record {record} begins a checkpoint after an entry of its tick")]
    CheckpointMidTick { record: u64 },
    /// The record of index `record`, from 0, does not decode, or the log refuses it.
    #[error("journal record {record}")]
    Record {
        record: u64,
        #[source]
        error: LogError,
    },
}

/// Why a journal does not publish its log. The server writes only records its log took, so each
/// is a fault of the journal's bytes, past the checks of its frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PublishError {
    /// The journal holds no header.
    #[error("the journal holds no header")]
    NoHeader,
    /// A checkpoint the journal begins has no record, so the log is not whole.
    #[error("a checkpoint the journal begins has no record")]
    Begun,
    /// The record of index `at`, from 0, is of no kind, or does not read.
    #[error("journal record {at} does not read")]
    Record { at: u64 },
}

/// Why a published log was not written: its journal does not publish it, or its writer failed.
#[derive(Debug, Error)]
pub enum PublishWriteError {
    #[error(transparent)]
    Journal(#[from] PublishError),
    #[error("could not write the published log")]
    Write(#[source] io::Error),
}
