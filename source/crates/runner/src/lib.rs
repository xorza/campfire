//! Loads packages, wires sim, the declared capabilities and script, and feeds inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod load_check;
mod mode_packages;
mod package;
mod runner;
mod script_facts;
mod session;

pub use error::{LoadError, LoadProblem, Place, Pool, StartError};
pub use mode_packages::ModePackages;
pub use runner::Runner;
pub use session::Session;

/// The tag of this engine release: what a package targets and a session's terms name.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");
