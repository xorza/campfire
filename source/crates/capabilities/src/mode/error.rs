use std::error::Error;
use std::fmt;

use crate::stats::stat::Stat;

/// Why a unit type's values do not make a unit. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitKitError {
    /// A stat its sections or its pools need is not declared.
    MissingStat(Stat),
    /// A stat overflows at level 1.
    Overflow(Stat),
    NotPositive(Stat),
    Negative(Stat),
    /// It has a `combat` section but not the life pool.
    NoLifePool,
}

/// Why the mode's setup does not start a match: its map or its teams name what the mode does
/// not have. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone)]
pub enum ModeError {
    /// More teams than a team index counts.
    TooManyTeams,
    /// A relation of a team to itself, or of a pair the relations name before.
    RepeatedRelation(String, String),
    /// Two teams, or two paths, share a name.
    RepeatedName(String),
    /// More players than the teams have slots.
    TooManyPlayers,
    UnknownTeam(String),
    UnknownUnitType(String),
    UnknownPath(String),
    /// A placed unit of the type walks from an end of no path.
    NoPathToWalk(String),
    /// A point of the map has the shape of the other metric's points.
    PointShape,
    /// The marker's region is not a box within the map's bounds of its metric, or the marker has
    /// a point too.
    Region(String),
    /// A path has no waypoint.
    EmptyPath(String),
    /// A point of the map is outside its bounds.
    OutOfBounds,
    /// The grid's cell is not positive or beyond the world's bound, or it makes more than 2²²
    /// cells.
    Grid,
    /// A unit type's value of a stat, or its gain a level, is not a number.
    StatValue,
}

impl fmt::Display for UnitKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitKitError::MissingStat(stat) => write!(f, "stat {stat} is not declared"),
            UnitKitError::Overflow(stat) => write!(f, "stat {stat} overflows"),
            UnitKitError::NotPositive(stat) => write!(f, "stat {stat} is not positive"),
            UnitKitError::Negative(stat) => write!(f, "stat {stat} is negative"),
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
            ModeError::TooManyPlayers => f.write_str("more players than slots"),
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
            ModeError::StatValue => {
                f.write_str("a unit type's stat value or gain a level is not a number")
            }
        }
    }
}

impl Error for ModeError {}
