//! Loads packages, wires sim, kits and script, and feeds inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod runner;
mod session;
mod stand_in_mode;

pub use runner::Runner;
pub use session::Session;
