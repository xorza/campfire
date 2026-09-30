//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod match_build;
mod runner;
mod session;

pub use error::StartError;
pub use runner::Runner;
pub use session::Session;
