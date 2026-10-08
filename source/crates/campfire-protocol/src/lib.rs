//! Session log format: headers, input chains, checkpoints, results; the connect handshake that
//! binds a player's identity to the transport; and the text of a key file. The crate does no IO:
//! the log frames its journal's records into a sink it is given.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod bytes;
mod checkpoint;
mod connect;
mod controller;
mod decoded;
mod delegation;
#[cfg(any(test, feature = "internals"))]
mod harness;
mod input_chain;
mod input_hash;
mod journal;
mod nsec;
mod player_input;
mod random_key;
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
mod slot_change;
mod slot_plan;
mod slot_start;
mod snapshot_fingerprint;

pub use crate::checkpoint::Checkpoint;
pub use crate::checkpoint::checkpoint_begun::CheckpointBegun;
pub use crate::checkpoint::error::CheckpointDecodeError;
pub use crate::checkpoint::log_carry::LogCarry;
pub use crate::connect::ConnectChallenge;
pub use crate::connect::certificate_hash::CertificateHash;
pub use crate::connect::error::ConnectError;
pub use crate::controller::Controller;
pub use crate::delegation::delegation_id::DelegationId;
pub use crate::delegation::error::{DelegationError, ScopeError};
pub use crate::delegation::seed_contribution::SeedContribution;
pub use crate::delegation::{Delegation, DelegationTerms};
pub use crate::input_chain::InputChain;
pub use crate::input_hash::InputHash;
pub use crate::journal::error::{JournalReplayError, NotJournal};
pub use crate::journal::journal_frames::JournalFrames;
pub use crate::journal::record_sink::RecordSink;
pub use crate::nsec::Nsec;
pub use crate::nsec::error::NsecError;
pub use crate::player_input::PlayerInput;
pub use crate::random_key::RandomKey;
pub use crate::receipt::error::ReceiptFileError;
pub use crate::receipt::{Receipt, SignedReceipt};
pub use crate::seed_chain::SeedChain;
pub use crate::seed_commitment::SeedCommitment;
pub use crate::server_input::error::ServerInputDecodeError;
pub use crate::server_input::{AfterLeave, InputPlace, LeaveReason, ServerInput};
pub use crate::server_seed::ServerSeed;
pub use crate::server_seeds::ServerSeeds;
pub use crate::session_id::SessionId;
pub use crate::session_log::SessionLog;
pub use crate::session_log::applied::Applied;
pub use crate::session_log::durable_head::DurableHead;
pub use crate::session_log::error::{
    CheckpointError, HeaderError, InputError, LogError, LogLoadError, ResultError, SeedError,
    ServerInputError,
};
pub use crate::session_log::session_header::SessionHeader;
pub use crate::session_private::SessionPrivate;
pub use crate::session_private::error::SessionPrivateError;
pub use crate::session_result::{Outcome, SessionResult};
pub use crate::session_terms::SessionTerms;
pub use crate::signature::Signature;
pub use crate::slot_change::{SlotChange, SlotChangeKind, Taken};
pub use crate::slot_plan::SlotPlan;
pub use crate::slot_start::SlotStart;
pub use crate::snapshot_fingerprint::SnapshotFingerprint;
/// The curve library the keys and signatures of this API are typed in, at the version it pins.
pub use secp256k1;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::harness::test_key::TestKey;
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::Criterion;

    use crate::input_chain;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        input_chain::bench::chain_head(c);
    }
}

#[cfg(test)]
mod tests;
