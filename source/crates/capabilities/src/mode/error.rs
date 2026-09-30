use std::error::Error;
use std::fmt;

use campfire_script::ScriptError;

use crate::stats::stat::Stat;

/// Why a unit type's values do not make a unit. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKitError {
    /// A stat its sections need is not declared.
    MissingStat(Stat),
    /// A stat overflows at level 1.
    Overflow(Stat),
    NotPositive(Stat),
    Negative(Stat),
    /// The windup is too long to count in ticks.
    TimeTooLarge,
    /// The attack's range is beyond a `Num`.
    Range,
    /// The attack's range or damage is negative, or its windup is not shorter than its period.
    Attack,
    /// The attack's projectile homes, and flies no faster than the move speed cap, so it might
    /// never catch its target.
    ProjectileNotFaster,
}

/// Why the mode's setup does not start a match: its map or its teams name what the mode does
/// not have. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone)]
pub enum ModeError {
    /// The mode script does not compile.
    Script(ScriptError),
    /// A team is named `neutral`, the name of the team neutral units spawn on.
    NeutralTeam,
    /// More teams than a team index counts, the neutral one included.
    TooManyTeams,
    /// Two teams, or two lanes, share a name.
    RepeatedName(String),
    /// More players than the teams have slots.
    TooManyPlayers,
    UnknownTeam(String),
    UnknownUnitType(String),
    UnknownLane(String),
    /// A playing team has no hero spawn.
    NoSpawn(String),
    /// A lane has no waypoint.
    EmptyLane(String),
    /// A point of the map is beyond the world's bound.
    OutOfBounds,
}

impl fmt::Display for UnitKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitKitError::MissingStat(stat) => write!(f, "stat {stat:?} is not declared"),
            UnitKitError::Overflow(stat) => write!(f, "stat {stat:?} overflows"),
            UnitKitError::NotPositive(stat) => write!(f, "stat {stat:?} is not positive"),
            UnitKitError::Negative(stat) => write!(f, "stat {stat:?} is negative"),
            UnitKitError::TimeTooLarge => f.write_str("windup too long to count in ticks"),
            UnitKitError::Range => f.write_str("attack range beyond a Num"),
            UnitKitError::Attack => f.write_str(
                "attack range or damage negative, or windup not shorter than the period",
            ),
            UnitKitError::ProjectileNotFaster => {
                f.write_str("attack projectile no faster than the move speed cap")
            }
        }
    }
}

impl Error for UnitKitError {}

impl fmt::Display for ModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModeError::Script(error) => write!(f, "mode script: {error}"),
            ModeError::NeutralTeam => f.write_str("a team is named neutral"),
            ModeError::TooManyTeams => f.write_str("more teams than a team index counts"),
            ModeError::RepeatedName(name) => write!(f, "{name:?} names two teams or lanes"),
            ModeError::TooManyPlayers => f.write_str("more players than slots"),
            ModeError::UnknownTeam(name) => write!(f, "no team {name:?}"),
            ModeError::UnknownUnitType(name) => write!(f, "no unit type {name:?}"),
            ModeError::UnknownLane(name) => write!(f, "no lane {name:?}"),
            ModeError::NoSpawn(team) => write!(f, "team {team:?} has no hero spawn"),
            ModeError::EmptyLane(name) => write!(f, "lane {name:?} has no waypoint"),
            ModeError::OutOfBounds => f.write_str("a map point is beyond the world's bound"),
        }
    }
}

impl Error for ModeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ModeError::Script(error) => Some(error),
            _ => None,
        }
    }
}
