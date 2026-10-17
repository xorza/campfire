use campfire_common::DeclaredName;
use thiserror::Error;

/// What does not hold of a `build` action, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BuildProblem {
    /// It aims at something but a point.
    #[error("action \"{0}\": a build aims at a point")]
    Aims(DeclaredName),
}
