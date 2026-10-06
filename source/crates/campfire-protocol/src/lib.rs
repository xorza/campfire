//! Session log format: headers, input chains, checkpoints, results; the connect handshake that
//! binds a player's identity to the transport; and the key files and durable files they rest on.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod checkpoint;
mod connect;
mod delegation;
mod durable_file;
mod input_chain;
mod input_hash;
mod journal;
mod key_file;
mod player_input;
mod seed_chain;
mod server_input;
mod server_seed;
mod server_seeds;
mod session_id;
mod session_log;
mod session_private;
mod session_result;
mod session_terms;
mod signature;
mod slot_change;
mod slot_plan;
mod slot_start;
mod snapshot_fingerprint;

pub use checkpoint::Checkpoint;
pub use checkpoint::error::CheckpointDecodeError;
pub use checkpoint::log_carry::LogCarry;
pub use connect::ConnectChallenge;
pub use connect::certificate_hash::CertificateHash;
pub use connect::error::ConnectError;

pub use delegation::error::{DelegationError, ScopeError};
pub use delegation::{Delegation, DelegationTerms};
pub use durable_file::DurableFile;
pub use durable_file::error::DurableError;
pub use input_chain::InputChain;
pub use input_hash::InputHash;
pub use journal::Journal;
pub use journal::error::{JournalError, JournalReplayError};
pub use journal::journal_file::JournalFile;
pub use journal::journal_frames::JournalFrames;
pub use journal::journal_watch::JournalWatch;
pub use key_file::KeyFile;
pub use key_file::error::KeyFileError;
pub use player_input::PlayerInput;
/// The curve library the keys and signatures of this API are typed in, at the version it pins.
pub use secp256k1;
pub use seed_chain::SeedChain;
pub use server_input::error::ServerInputDecodeError;
pub use server_input::{AfterLeave, InputPlace, LeaveReason, ServerInput};
pub use server_seed::{SeedCommitment, ServerSeed};
pub use server_seeds::ServerSeeds;
pub use session_id::SessionId;
pub use session_log::error::{
    CheckpointError, HeaderError, InputError, LogError, ResultError, SeedError, ServerInputError,
};
pub use session_log::{Applied, SessionHeader, SessionLog};
pub use session_private::SessionPrivate;
pub use session_result::{Outcome, SessionResult};
pub use session_terms::SessionTerms;
pub use signature::Signature;
pub use slot_change::{SlotChange, SlotChangeKind, Taken};
pub use slot_plan::SlotPlan;
pub use slot_start::SlotStart;
pub use snapshot_fingerprint::SnapshotFingerprint;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::input_chain::bench::chain_head_signature;
}
