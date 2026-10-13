//! The build settings of the sim's schedules. A proxy crate: the calls that set a schedule's build
//! settings, which `clippy.toml` denies every other crate as they can switch off the ambiguity and
//! hierarchy checks, are made here alone.

#![expect(
    clippy::disallowed_methods,
    reason = "the one crate that sets a schedule's build settings, which clippy.toml denies the others"
)]

mod strict_schedule;

pub use crate::strict_schedule::StrictSchedule;
