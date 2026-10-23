use campfire_common::StateHash;
use campfire_protocol::{
    CheckpointError, LogLoadError, Outcome, PlayerSlotChange, SeedError, ServerInputError,
};
use campfire_script::CallError;
use campfire_sim::SnapshotError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::player_slot_rules::error::PlayerSlotRuleError;
use crate::session_rules::error::TermsError;

/// Why a session log does not start a match. A published log is untrusted, and so are packages,
/// so each is an expected failure.
#[derive(Debug, Error)]
pub enum StartError {
    /// The log gives no segment seed.
    #[error(transparent)]
    Seed(SeedError),
    #[error(transparent)]
    Terms(TermsError),
    /// A change of a player slot's controller the mode's `[players]` does not allow.
    #[error("player slot {} in tick {}", .change.player_slot.get(), .change.tick)]
    PlayerSlotRule {
        change: PlayerSlotChange,
        #[source]
        error: PlayerSlotRuleError,
    },
    /// The mode script's `on_match_start` failed.
    #[error("the mode's start failed")]
    MatchStart(#[source] CallError),
    /// The log's file starts at the checkpoint of segment `segment`, whose snapshot a match
    /// starts from, not from tick 0.
    #[error("the log starts at the checkpoint of segment {segment}")]
    AtCheckpoint { segment: u32 },
}

/// Why a snapshot does not hold what a checkpoint record says of it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SnapshotCheckError {
    /// Its fingerprint is not the record's.
    #[error("the snapshot is not the one its checkpoint names")]
    Fingerprint,
    /// It does not restore.
    #[error("the snapshot does not restore")]
    Restore(#[source] SnapshotError),
    /// It restores to another state than the record's.
    #[error("the snapshot restores to state {restored}, and its checkpoint records {logged}")]
    Hash {
        logged: StateHash,
        restored: StateHash,
    },
}

/// Why a session log and a snapshot do not resume a match from a checkpoint. A restore reads
/// both from the disk, and a verifier from a published log, so each is an expected failure.
#[derive(Debug, Error)]
pub enum ResumeError {
    #[error(transparent)]
    Start(StartError),
    /// The log holds no checkpoint that starts the segment.
    #[error("the log holds no such checkpoint")]
    NoCheckpoint,
    #[error(transparent)]
    Snapshot(SnapshotCheckError),
    /// The log does not load the save, as a test loads one.
    #[cfg(feature = "internals")]
    #[error("the save does not load")]
    Load(#[source] LogLoadError),
}

/// Why a session does not load a save in place. The snapshot is read from the disk, and the log
/// may have no room for the load, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SaveLoadError {
    #[error(transparent)]
    Log(LogLoadError),
    /// The snapshot is not the one the save's checkpoint names.
    #[error("the snapshot is not the one its save names")]
    Fingerprint,
}

/// Why a session begins no checkpoint at a boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckpointBeginError {
    /// The seed chain has no segment after the log's last: the session runs out of chain.
    #[error("the seed chain has no segment after the last")]
    PastSeeds,
    #[error(transparent)]
    Log(CheckpointError),
}

/// Why a session's log refused a bot's input, which the server drops, as a bot's payload changes
/// no player slot's controller for the mode's rules to refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotInputDrop {
    /// The player slot's inputs fill every tick up to the session's max input lead.
    AheadOfTime,
    /// The session log is full: the session ends at the next boundary.
    LogFull,
}

/// Why a session refused a server input: the log's structure, or the mode's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ServerInputRefusal {
    #[error(transparent)]
    Log(ServerInputError),
    #[error(transparent)]
    Rule(PlayerSlotRuleError),
}

/// Why a log's result does not hold for the state its replay ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ResultMismatch {
    /// The result's state hash is not the replay's.
    #[error("the result's state hash {logged} is not the replay's {replayed}")]
    Hash {
        logged: StateHash,
        replayed: StateHash,
    },
    /// The result is not the outcome the mode ended the match with.
    #[error("the result says {logged:?}, and the match ended as {ended:?}")]
    Outcome { logged: Outcome, ended: Outcome },
}
