use campfire_capabilities::CallError;
use campfire_common::StateHash;
use campfire_package::StoreError;
use campfire_protocol::{
    CheckpointError, LogLoadError, Outcome, SeedError, ServerInputError, SlotChange,
};
use campfire_sim::SnapshotError;
use thiserror::Error;

use crate::session_rules::error::TermsError;
use crate::slot_rules::error::SlotRuleError;

/// Why a session log does not start a match. A published log is untrusted, and so are packages,
/// so each is an expected failure.
#[derive(Debug, Error)]
pub enum StartError {
    /// The log gives no segment seed.
    #[error(transparent)]
    Seed(SeedError),
    #[error(transparent)]
    Terms(TermsError),
    /// The store does not give the packages the terms name.
    #[error(transparent)]
    Packages(StoreError),
    /// A change of a slot's controller the mode's `[players]` does not allow.
    #[error("slot {} in tick {}", .change.slot.get(), .change.tick)]
    SlotRule {
        change: SlotChange,
        #[source]
        error: SlotRuleError,
    },
    /// The mode script's `on_match_start` failed.
    #[error("the mode's start failed")]
    MatchStart(#[source] CallError),
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
    /// The snapshot is not the one the checkpoint fingerprints.
    #[error("the snapshot is not the one the checkpoint fingerprints")]
    Fingerprint,
    #[error("the snapshot does not restore")]
    Snapshot(#[source] SnapshotError),
    /// The snapshot restores to another state hash than the checkpoint's.
    #[error("the snapshot restores to another state than the checkpoint's")]
    StateHash,
    /// The log does not load the save.
    #[error("the save does not load")]
    Load(#[source] LogLoadError),
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

/// Why a session refused a server input: the log's structure, or the mode's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ServerInputRefused {
    #[error(transparent)]
    Log(ServerInputError),
    #[error(transparent)]
    Rule(SlotRuleError),
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
