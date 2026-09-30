//! Session log format: headers, input chains, checkpoints, results.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod chain_signature;
mod delegation;
mod input_chain;
mod input_hash;
mod player_input;
mod player_slot;
mod server_seed;
mod session_id;
mod session_log;
mod session_terms;

pub use chain_signature::ChainSignature;
pub use delegation::delegation_tag::DelegationTag;
pub use delegation::error::DelegationError;
pub use delegation::{Delegation, DelegationTerms};
pub use input_chain::InputChain;
pub use input_hash::InputHash;
pub use player_input::PlayerInput;
pub use player_slot::PlayerSlot;
/// The curve library the keys and signatures of this API are typed in, at the version it pins.
pub use secp256k1;
pub use server_seed::{SeedCommitment, ServerSeed};
pub use session_id::SessionId;
pub use session_log::error::{HeaderError, InputError, LogError, SeedError};
pub use session_log::{Applied, SessionHeader, SessionLog, SessionPlayer};
pub use session_terms::SessionTerms;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::input_chain::bench::chain_head_signature;
}
