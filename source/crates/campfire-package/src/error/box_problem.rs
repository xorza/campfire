use thiserror::Error;

/// What is wrong with a unit type's box body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BoxProblem {
    /// The type declares a move speed: a box never walks.
    #[error("a box never walks, and the type declares a move speed")]
    Walks,
    /// The map is spatial: a box lies on the ground plane of a planar map.
    #[error("a box lies on a planar map, and the map is spatial")]
    Spatial,
}
