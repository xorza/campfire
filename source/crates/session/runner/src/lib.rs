//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod events;
mod match_build;
mod replay;
mod session;
mod session_rules;
mod slot_rules;

pub use crate::events::script_call_failed::ScriptCallFailed;
pub use crate::match_build::MatchBuild;
pub use crate::replay::Replay;
pub use crate::replay::error::ReplayError;
pub use crate::session::Session;
pub use crate::session::error::{
    CheckpointBeginError, ResultMismatch, ResumeError, ServerInputRefused, SnapshotCheckError,
    StartError,
};
pub use crate::session::session_source::{ResumeAt, SessionSource};
pub use crate::session::session_world::SessionWorld;
pub use crate::session_rules::error::TermsError;
pub use crate::session_rules::{CheckedTerms, HostChoices, SessionRules};
pub use crate::slot_rules::error::SlotRuleError;
