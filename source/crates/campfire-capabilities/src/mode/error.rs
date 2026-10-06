use thiserror::Error;

use crate::values::declared_name::DeclaredName;
use crate::values::polygon::error::PolygonError;
use crate::values::stat::Stat;

/// Why a unit type's values do not make a unit. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnitKitError {
    /// A stat its pools need is not one it gives a value.
    #[error("stat {0} is not declared")]
    MissingStat(Stat),
    /// A pool's maximum is not positive at level 1.
    #[error("stat {0} is not positive")]
    NotPositive(Stat),
    /// It has a `combat` section but not the life pool.
    #[error("a unit type with combat lacks the life pool")]
    NoLifePool,
    /// It has the life pool but no `combat` section.
    #[error("a unit type with the life pool lacks combat")]
    NoCombat,
}

/// Why the mode's teams, relations or map do not load: they name what the mode does not have.
/// Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModeError {
    /// More teams than a team index counts.
    #[error("more teams than a team index counts")]
    TooManyTeams,
    /// More teams than a map with a vision grid holds.
    #[error("more teams than a map with a vision grid holds")]
    TooManyVisionTeams,
    /// A relation of a team to itself, or of a pair the relations name before.
    #[error("a relation of \"{0}\" and \"{1}\" again, or of a team to itself")]
    RepeatedRelation(DeclaredName, DeclaredName),
    /// Two teams, two paths or two markers share a name.
    #[error("\"{0}\" names two teams, paths or markers")]
    RepeatedName(DeclaredName),
    #[error("no team \"{0}\"")]
    UnknownTeam(DeclaredName),
    #[error("no unit type \"{0}\"")]
    UnknownUnitType(DeclaredName),
    #[error("no path \"{0}\"")]
    UnknownPath(DeclaredName),
    /// A placed unit of the type walks from an end of no path.
    #[error("a placed \"{0}\" walks from an end of no path")]
    NoPathToWalk(DeclaredName),
    /// A point of the map has the shape of the other metric's points.
    #[error("a point of the map does not fit its metric")]
    PointShape,
    /// The marker's region is not a box within the map's bounds of its metric, or the marker has
    /// a point too.
    #[error("marker \"{0}\": the region is no box within the bounds, or has a point too")]
    Region(DeclaredName),
    /// A path has no waypoint.
    #[error("path \"{0}\" has no waypoint")]
    EmptyPath(DeclaredName),
    /// A point of the map is outside its bounds.
    #[error("a map point is outside the map's bounds")]
    OutOfBounds,
    /// The grid's cell is not positive or beyond the world's bound, or it makes more than 2²²
    /// cells.
    #[error("the grid needs a positive cell within the world's bound and at most 2²² cells")]
    Grid,
    /// A wall names a layer the mode does not declare.
    #[error("no layer \"{0}\"")]
    UnknownLayer(DeclaredName),
    /// The points of wall `wall`, counted from 0, make no simple polygon.
    #[error("wall {wall}")]
    Wall {
        wall: usize,
        #[source]
        problem: PolygonError,
    },
    /// The points of brush `brush`, counted from 0, make no simple polygon.
    #[error("brush {brush}")]
    Brush {
        brush: usize,
        #[source]
        problem: PolygonError,
    },
}
