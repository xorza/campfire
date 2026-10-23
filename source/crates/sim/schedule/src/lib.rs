//! The build settings and the executor of the engine's schedules. A proxy crate: the calls that set
//! a schedule's build settings, which can switch off the ambiguity and hierarchy checks, and its
//! executor, which can run its systems on other threads, are made here alone, as `clippy.toml`
//! denies them every other crate.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::disallowed_methods,
    reason = "the one crate that sets a schedule's build settings and executor, which clippy.toml denies the others"
)]

mod schedule_ext;
mod schedules_ext;

pub use crate::schedule_ext::ScheduleExt;
pub use crate::schedules_ext::SchedulesExt;

#[cfg(feature = "internals")]
pub mod internals {
    #[expect(
        clippy::module_name_repetitions,
        reason = "a trait's extension for tests and benches is `<Trait>Internals` (AGENTS.md)"
    )]
    pub use crate::schedule_ext::internals::ScheduleExtInternals;
}
