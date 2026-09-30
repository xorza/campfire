//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod match_build;
#[cfg(feature = "internals")]
mod reference_3v3;
mod runner;
mod session;

pub use error::StartError;
#[cfg(feature = "internals")]
pub use reference_3v3::Reference3v3;
pub use runner::Runner;
pub use session::Session;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::runner::bench::tick_3v3;
}
