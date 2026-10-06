//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod events;
#[cfg(feature = "internals")]
mod harness;
mod input_rules;
mod match_build;
mod runner;
mod session;
mod session_rules;
mod slot_rules;

pub use error::{
    CheckpointBeginError, ResultMismatch, ResumeError, ServerInputRefused, SlotRuleError,
    StartError, TermsError,
};
pub use events::script_call_failed::ScriptCallFailed;
pub use input_rules::InputRules;
pub use runner::Runner;
pub use session::Session;
pub use session_rules::SessionRules;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::runner::bench::tick_3v3;
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::harness::arena::Arena;
    pub use crate::harness::copy_check::CopyCheck;
    pub use crate::harness::fixed_match::FixedMatch;
    pub use crate::harness::fixed_session::FixedSession;
    pub use crate::harness::golden::Golden;
    pub use crate::harness::hash_trail::{Difference, HashTrail};
    pub use crate::harness::match_units::MatchUnits;
    pub use crate::harness::proving_match::ProvingMatch;
    pub use crate::harness::reference_3v3::Reference3v3;
    pub use crate::harness::restore_target::RestoreTarget;
}
