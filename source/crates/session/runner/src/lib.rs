//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod events;
mod match_build;
mod replay;
mod seat_rules;
mod session;
mod session_rules;

pub use crate::events::script_call_failed::ScriptCallFailed;
pub use crate::match_build::MatchBuild;
pub use crate::replay::Replay;
pub use crate::replay::error::ReplayError;
pub use crate::seat_rules::error::SeatRuleError;
pub use crate::session::Session;
pub use crate::session::error::{
    CheckpointBeginError, ResultMismatch, ResumeError, ServerInputRefusal, SnapshotCheckError,
    StartError,
};
pub use crate::session::session_source::{ResumeAt, SessionSource, UncheckedSource};
pub use crate::session::session_world::SessionWorld;
pub use crate::session_rules::error::TermsError;
pub use crate::session_rules::{CheckedTerms, HostChoices, SessionRules};

/// What the test harness builds an arena of.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::match_build::internals::ArenaSetup;
}
