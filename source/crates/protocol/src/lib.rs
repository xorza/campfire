//! Session log format: headers, input chains, checkpoints, results.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod input_chain;
mod input_hash;
mod player_input;
mod player_slot;
mod server_seed;
mod session_log;

pub use input_chain::InputChain;
pub use input_hash::InputHash;
pub use player_input::PlayerInput;
pub use player_slot::PlayerSlot;
pub use server_seed::{SeedCommitment, ServerSeed};
pub use session_log::error::{InputError, SeedError};
pub use session_log::{Applied, SessionHeader, SessionLog, SessionPlayer};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::input_chain::bench::chain_head_signature;
}
