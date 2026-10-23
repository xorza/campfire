use campfire_common::DeclaredName;
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

/// Why a unit type breaks a rule of the units. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnitTypeError {
    /// It holds a tag only the engine gives.
    #[error("the tag {0}, which only the engine gives")]
    EngineTag(DeclaredName),
    /// A delivery type with no delivery's section, more than one, or a section no delivery type
    /// holds.
    #[error("a delivery type holds one delivery's section, and beside it only those of any type")]
    NotDelivery,
    /// A box body on a spatial map, where a box lies on the ground plane of a planar map.
    #[error("a box lies on a planar map, and the map is spatial")]
    BoxOnSpatial,
    /// A `collision` beside `variations`, which give the type's bodies in its place.
    #[error("a type of `variations` takes its bodies of them, and holds no `collision`")]
    VariedCollision,
    /// `variations` of none, or of more than 256.
    #[error("a type's `variations` are 1 to 256, not {0}")]
    VariationCount(usize),
    /// Variations on layers apart, where one type moves on one layer.
    #[error("a type's `variations` lie on one layer")]
    VariationLayers,
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

/// Why a map's ground does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GroundLoadError {
    /// The map gives a `[ground]` but no height grid.
    #[error("a [ground] with no height grid")]
    NoHeights,
    /// A spatial map gives a `[ground]`, which the sim of a spatial map reads none of.
    #[error("a [ground] on a spatial map")]
    Spatial,
    /// The height grid and the `[ground]` make no ground.
    #[error(transparent)]
    Ground(GroundError),
}

/// Why a point of a map has no place on it. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MapPointError {
    /// It has the shape of the other metric's points.
    #[error("a point of the map does not fit its metric")]
    Shape,
    /// It lies outside the map's bounds.
    #[error("a map point is outside the map's bounds")]
    OutOfBounds,
}
