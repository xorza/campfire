//! Session log format: headers, input chains, checkpoints, results; and the connect handshake
//! that binds a player's identity to the transport.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod connect;
mod delegation;
mod fingerprint;
mod input_chain;
mod input_hash;
mod player_input;
mod seed_chain;
mod server_seed;
mod session_id;
mod session_log;
mod session_terms;
mod signature;

pub use connect::ConnectChallenge;
pub use connect::certificate_hash::CertificateHash;
pub use connect::error::ConnectError;
pub use delegation::delegation_tag::DelegationTag;
pub use delegation::error::{DelegationError, ScopeError};
pub use delegation::{Delegation, DelegationTerms};
pub use fingerprint::Fingerprint;
pub use input_chain::InputChain;
pub use input_hash::InputHash;
pub use player_input::PlayerInput;
/// The curve library the keys and signatures of this API are typed in, at the version it pins.
pub use secp256k1;
pub use seed_chain::SeedChain;
pub use server_seed::{SeedCommitment, ServerSeed};
pub use session_id::SessionId;
pub use session_log::error::{HeaderError, InputError, LogError, SeedError};
pub use session_log::{Applied, SessionHeader, SessionLog};
pub use session_terms::SessionTerms;
pub use signature::Signature;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::input_chain::bench::chain_head_signature;
}
