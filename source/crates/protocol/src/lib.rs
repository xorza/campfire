//! Session log format: headers, input chains, checkpoints, results.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod input_hash;
mod player_input;
mod player_slot;
mod session_log;

pub use input_hash::InputHash;
pub use player_input::PlayerInput;
pub use player_slot::PlayerSlot;
pub use session_log::error::InputError;
pub use session_log::{Applied, SessionHeader, SessionLog};
