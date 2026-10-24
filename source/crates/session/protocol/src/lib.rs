//! Session log format: headers, input chains, checkpoints, results; the connect handshake that
//! binds a player's identity to the transport; and the text of a key file. The crate does no IO:
//! the log frames its journal's records into a sink it is given.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod bytes;
mod checkpoint;
mod connect;
mod control;
mod delegation;
mod entropy;
#[cfg(feature = "internals")]
mod harness;
mod input_chain;
mod input_hash;
mod input_rules;
mod main_key;
mod nsec;
mod player_input;
mod player_slot_change;
mod player_slot_plan;
mod player_slot_start;
mod receipt;
mod seed_chain;
mod seed_commitment;
mod server_input;
mod server_seed;
mod server_seeds;
mod session_id;
mod session_log;
mod session_private;
mod session_result;
mod session_terms;
mod signature;
mod signed;
mod signer;
mod snapshot_fingerprint;
mod spill;
mod stamp_count;
mod verifier;

pub use crate::checkpoint::Checkpoint;
pub use crate::checkpoint::checkpoint_begun::CheckpointBegun;
pub use crate::checkpoint::error::CheckpointDecodeError;
pub use crate::checkpoint::log_carry::LogCarry;
pub use crate::connect::error::ConnectError;
pub use crate::connect::{ConnectAnswer, ConnectChallenge, ConnectChallengeMeaning};
pub use crate::control::{Control, Controller};
pub use crate::delegation::delegation_id::{DelegationId, DelegationIdMeaning};
pub use crate::delegation::delegation_tag::DelegationTag;
pub use crate::delegation::error::{DelegationError, EventError, ScopeError};
pub use crate::delegation::seed_contribution::{SeedContribution, SeedContributionMeaning};
pub use crate::delegation::{Delegation, DelegationTerms};
pub use crate::entropy::Entropy;
pub use crate::input_chain::{InputChain, SignedHead};
pub use crate::input_hash::{InputHash, InputHashMeaning};
pub use crate::input_rules::InputRules;
pub use crate::main_key::{MainKey, MainKeyMeaning};
pub use crate::nsec::error::NsecError;
pub use crate::nsec::{MAX_NSEC_FILE_LEN, decode_nsec, encode_nsec};
pub use crate::player_input::PlayerInput;
pub use crate::player_slot_change::{PlayerSlotChange, PlayerSlotChangeKind, Taken};
pub use crate::player_slot_plan::PlayerSlotPlan;
pub use crate::player_slot_plan::error::NotPlayerSlotPlan;
pub use crate::player_slot_start::PlayerSlotStart;
pub use crate::receipt::{Receipt, SignedReceipt};
pub use crate::seed_chain::SeedChain;
pub use crate::seed_commitment::{SeedCommitment, SeedCommitmentMeaning};
pub use crate::server_input::error::ServerInputDecodeError;
pub use crate::server_input::{InputPlace, LeaveReason, ServerInput};
pub use crate::server_seed::ServerSeed;
pub use crate::server_seeds::ServerSeeds;
pub use crate::session_id::{SessionId, SessionIdMeaning};
pub use crate::session_log::applied::Applied;
pub use crate::session_log::durable_head::DurableHead;
pub use crate::session_log::error::{
    CarryFlaw, CheckpointError, HeaderError, InputError, JournalReplayError, LogError,
    LogLoadError, PublishError, PublishWriteError, ResultError, SeedError, ServerInputError,
};
pub use crate::session_log::journal::Journal;
pub use crate::session_log::journal::error::{JournalStreamError, NotJournal};
pub use crate::session_log::journal::journal_frames::JournalFrames;
pub use crate::session_log::journal::record_sink::RecordSink;
pub use crate::session_log::log_replay::LogReplay;
pub use crate::session_log::player_packet::PlayerPacket;
pub use crate::session_log::session_header::SessionHeader;
pub use crate::session_log::{AppliedInput, ServerServe, SessionLog};
pub use crate::session_private::SessionPrivate;
pub use crate::session_result::{Outcome, SessionResult};
pub use crate::session_terms::SessionTerms;
pub use crate::signature::Signature;
pub use crate::signed::{SignatureDomain, Signed};
pub use crate::signer::Signer;
pub use crate::snapshot_fingerprint::{SnapshotFingerprint, SnapshotFingerprintMeaning};
pub use crate::verifier::Verifier;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::harness::test_key::{PlayerKeys, TestKey};
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::{input_chain, server_seeds, session_log};

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        input_chain::bench::input_chain(c);
        server_seeds::bench::server_seeds(c);
        session_log::bench::record_tick(c);
    }
}

#[cfg(test)]
mod tests;
