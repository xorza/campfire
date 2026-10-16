use thiserror::Error;

/// Why the match's unit types or tags do not load. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TypesError {
    /// More tags than a match holds.
    #[error("more tags than a match holds")]
    TooManyTags,
    /// More unit types than a match holds.
    #[error("more unit types than a match holds")]
    TooManyTypes,
    /// Two unit types of one scope by one name, as an avatar named as one of the mode's types.
    #[error("unit type {0} twice")]
    Repeated(String),
}

/// Why a map's ground does not load: its coarse cells do not fit its height grid. Packages are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GroundError {
    /// A coarse cell smaller than the grid's cell.
    #[error("the ground's coarse cell is smaller than its grid's cell")]
    CoarseCell,
    /// A span of the coarse cells that spans nothing on an axis.
    #[error("the ground's coarse cells span nothing on an axis")]
    CoarseSpan,
    /// A span or a coarse cell past the world's bound.
    #[error("the ground's coarse cells lie past the world's bound")]
    CoarseBound,
    /// More coarse cells than the grid has samples.
    #[error("the ground has more coarse cells than its grid has samples")]
    CoarseCount,
}
