//! Loads packages, wires sim, the declared capabilities and script, and feeds inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod runner;
mod session;
mod stand_in_mode;

pub use error::StartError;
pub use runner::Runner;
pub use session::Session;
pub use stand_in_mode::StandInMode;
