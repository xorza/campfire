use std::error::Error;
use std::fmt;

use crate::stats::stat::Stat;
use crate::values::declared_name::DeclaredName;

/// Why a unit type's values do not make a unit. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitKitError {
    /// A stat its pools need is not one it gives a value.
    MissingStat(Stat),
    /// A pool's maximum is not positive at level 1.
    NotPositive(Stat),
    /// It has a `combat` section but not the life pool.
    NoLifePool,
}

/// Why the mode's teams, relations or map do not load: they name what the mode does not have.
/// Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeError {
    /// More teams than a team index counts.
    TooManyTeams,
    /// A relation of a team to itself, or of a pair the relations name before.
    RepeatedRelation(DeclaredName, DeclaredName),
    /// Two teams, or two paths, share a name.
    RepeatedName(DeclaredName),
    UnknownTeam(DeclaredName),
    UnknownUnitType(DeclaredName),
    UnknownPath(DeclaredName),
    /// A placed unit of the type walks from an end of no path.
    NoPathToWalk(DeclaredName),
    /// A point of the map has the shape of the other metric's points.
    PointShape,
    /// The marker's region is not a box within the map's bounds of its metric, or the marker has
    /// a point too.
    Region(DeclaredName),
    /// A path has no waypoint.
    EmptyPath(DeclaredName),
    /// A point of the map is outside its bounds.
    OutOfBounds,
    /// The grid's cell is not positive or beyond the world's bound, or it makes more than 2²²
    /// cells.
    Grid,
}

impl fmt::Display for UnitKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitKitError::MissingStat(stat) => write!(f, "stat {stat} is not declared"),
            UnitKitError::NotPositive(stat) => write!(f, "stat {stat} is not positive"),
            UnitKitError::NoLifePool => f.write_str("a unit type with combat lacks the life pool"),
        }
    }
}

impl Error for UnitKitError {}

impl fmt::Display for ModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModeError::TooManyTeams => f.write_str("more teams than a team index counts"),
            ModeError::RepeatedRelation(a, b) => {
                write!(
                    f,
                    "a relation of {a:?} and {b:?} again, or of a team to itself"
                )
            }
            ModeError::RepeatedName(name) => write!(f, "{name:?} names two teams or paths"),
            ModeError::UnknownTeam(name) => write!(f, "no team {name:?}"),
            ModeError::UnknownUnitType(name) => write!(f, "no unit type {name:?}"),
            ModeError::UnknownPath(name) => write!(f, "no path {name:?}"),
            ModeError::NoPathToWalk(unit_type) => {
                write!(f, "a placed {unit_type:?} walks from an end of no path")
            }
            ModeError::PointShape => f.write_str("a point of the map does not fit its metric"),
            ModeError::Region(marker) => {
                write!(
                    f,
                    "marker {marker:?}: the region is no box within the bounds, or has a point too"
                )
            }
            ModeError::EmptyPath(name) => write!(f, "path {name:?} has no waypoint"),
            ModeError::OutOfBounds => f.write_str("a map point is outside the map's bounds"),
            ModeError::Grid => f.write_str(
                "the grid needs a positive cell within the world's bound and at most 2²² cells",
            ),
        }
    }
}

impl Error for ModeError {}
