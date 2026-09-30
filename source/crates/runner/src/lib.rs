//! Loads packages, wires sim, the declared capabilities and script, and feeds inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

use campfire_capabilities::Version;

mod error;
mod load_check;
mod match_build;
mod mode_packages;
mod package;
mod runner;
mod script_facts;
mod session;

pub use error::{CtxMisuse, LoadError, LoadProblem, Place, StartError};
pub use mode_packages::ModePackages;
pub use runner::Runner;
pub use session::Session;

/// The tag of this engine release: what a package targets and a session's terms name.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");

/// `RELEASE` as the version a package's `engine` names.
const RELEASE_VERSION: Version = match Version::parse(RELEASE) {
    Some(version) => version,
    None => panic!("the crate version is major.minor.patch"),
};
