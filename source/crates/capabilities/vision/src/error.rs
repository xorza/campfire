use campfire_geometry::PolygonError;
use campfire_units::MapPointError;
use thiserror::Error;

/// Why the map's vision grid breaks a rule of vision. Packages are untrusted, so each is an
/// expected failure; brushes count from 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VisionDataError {
    /// The mode declares vision, and its map has no grid for sight to reveal.
    #[error("the mode declares vision, and its map has no grid")]
    NoGrid,
    /// The grid's cell is not positive or beyond the world's bound, or it makes more than 2²²
    /// cells.
    #[error("the grid needs a positive cell within the world's bound and at most 2²² cells")]
    Grid,
    /// A point of a brush has no place on the map.
    #[error(transparent)]
    Point(#[from] MapPointError),
    /// The points of brush `brush` make no simple polygon.
    #[error("brush {brush}")]
    Brush {
        brush: usize,
        #[source]
        problem: PolygonError,
    },
    /// More teams than a map with a vision grid holds.
    #[error("more teams than a map with a vision grid holds")]
    TooManyTeams,
    /// More cells times teams than a match's counts of sight may take
    /// (`VisionGrid::MAX_CELL_TEAMS`).
    #[error("the grid's cells times the match's teams pass what its sight's counts may take")]
    TooLarge,
}
