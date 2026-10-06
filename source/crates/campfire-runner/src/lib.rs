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
#[cfg(feature = "internals")]
mod hash_trail;
mod input_rules;
mod match_build;
#[cfg(feature = "internals")]
mod match_units;
#[cfg(feature = "internals")]
mod proving_match;
#[cfg(feature = "internals")]
mod reference_3v3;
#[cfg(feature = "internals")]
mod restore_target;
mod runner;
mod script_call_failed;
#[cfg(feature = "internals")]
mod scripted;
mod session;
mod session_rules;
mod slot_rules;

pub use error::{ResultMismatch, ServerInputRefused, SlotRuleError, StartError, TermsError};
pub use input_rules::InputRules;
pub use runner::Runner;
pub use script_call_failed::ScriptCallFailed;
pub use session::Session;
pub use session_rules::SessionRules;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::runner::bench::tick_3v3;
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::arena::Arena;
    pub use crate::fixed_match::FixedMatch;
    pub use crate::fixed_session::FixedSession;
    pub use crate::golden::Golden;
    pub use crate::hash_trail::{Difference, HashTrail};
    pub use crate::match_units::MatchUnits;
    pub use crate::proving_match::ProvingMatch;
    pub use crate::reference_3v3::Reference3v3;
    pub use crate::restore_target::RestoreTarget;
}
