use std::num::NonZeroU32;

use thiserror::Error;

/// Why session terms do not name a session of a mode's packages on this release. Terms come from
/// a server or a published log, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TermsError {
    /// The terms name another engine release than this one.
    #[error("the session runs on engine release {0:?}, not this one")]
    OtherRelease(String),
    /// The packages are another mode than the one the terms name.
    #[error("the mode is not the one the session names")]
    OtherMode,
    /// The packages' dependencies are others than the ones the terms name.
    #[error("the dependencies are not the ones the session names")]
    OtherDependencies,
    /// The tick rate the terms fix is outside the mode's range.
    #[error("{0} ticks a second is outside the mode's range")]
    TickRate(NonZeroU32),
    /// The terms plan `slots` slots, none or more than the `most` the mode's teams have.
    #[error("{slots} slots, and the mode's teams have {most}")]
    Slots { slots: u64, most: u64 },
}
