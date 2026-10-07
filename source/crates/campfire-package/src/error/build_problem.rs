use campfire_capabilities::DeclaredName;
use thiserror::Error;

/// What does not hold of a `build` action, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BuildProblem {
    /// It aims at something but a point.
    #[error("action \"{0}\": a build aims at a point")]
    Aims(DeclaredName),
    /// Its range is global, which no builder walks into.
    #[error("action \"{0}\": a build's range is in meters")]
    Global(DeclaredName),
    /// Its unit type has no box body, which a building has.
    #[error("action \"{0}\": a build places a unit type with a box body")]
    NoBox(DeclaredName),
    /// Its `start_life` is 0, or none for a building with the mode's life pool.
    #[error("action \"{0}\": a build's site starts with a share of its life above 0")]
    StartLife(DeclaredName),
}
