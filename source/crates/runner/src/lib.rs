//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

#[cfg(feature = "internals")]
mod arena;
mod error;
#[cfg(feature = "internals")]
mod fixed_match;
#[cfg(feature = "internals")]
mod fixed_session;
#[cfg(feature = "internals")]
mod golden;
mod input_rules;
mod match_build;
#[cfg(feature = "internals")]
mod proving_match;
#[cfg(feature = "internals")]
mod reference_3v3;
mod runner;
mod session;
mod session_rules;

#[cfg(feature = "internals")]
pub use arena::Arena;
pub use error::{StartError, TermsError};
#[cfg(feature = "internals")]
pub use fixed_match::FixedMatch;
#[cfg(feature = "internals")]
pub use fixed_session::FixedSession;
#[cfg(feature = "internals")]
pub use golden::Golden;
pub use input_rules::InputRules;
#[cfg(feature = "internals")]
pub use proving_match::ProvingMatch;
#[cfg(feature = "internals")]
pub use reference_3v3::Reference3v3;
pub use runner::Runner;
pub use session::Session;
pub use session_rules::SessionRules;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::runner::bench::tick_3v3;
}
