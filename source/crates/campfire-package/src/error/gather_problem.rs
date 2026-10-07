use campfire_capabilities::DeclaredName;
use thiserror::Error;

/// What does not hold of a `gather` action, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GatherProblem {
    /// It aims at something but a unit its filter selects.
    #[error("action \"{0}\": a gather aims at a unit its filter selects")]
    Aims(DeclaredName),
    /// Its range is global, which no worker walks into.
    #[error("action \"{0}\": a gather's range is in meters")]
    Global(DeclaredName),
    /// Its bounce is no distance from 0.
    #[error("action \"{0}\": a gather's bounce is a distance from 0")]
    Bounce(DeclaredName),
}
