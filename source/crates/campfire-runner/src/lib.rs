//! Builds and runs matches: wires the sim, the declared capabilities and the mode of checked
//! packages, and feeds a session log's inputs.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod events;
#[cfg(feature = "internals")]
mod harness;
mod input_rules;
mod match_build;
mod runner;
mod session;
mod session_rules;
mod slot_rules;

pub use crate::events::script_call_failed::ScriptCallFailed;
pub use crate::input_rules::InputRules;
pub use crate::runner::Runner;
pub use crate::session::Session;
pub use crate::session::error::{
    CheckpointBeginError, ResultMismatch, ResumeError, ServerInputRefused, StartError,
};
pub use crate::session_rules::SessionRules;
pub use crate::session_rules::error::TermsError;
pub use crate::slot_rules::error::SlotRuleError;

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::Criterion;

    use crate::harness::reference_3v3;
    use crate::runner;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        runner::bench::server(c);
        reference_3v3::bench::script_view(c);
    }
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
