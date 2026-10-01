use std::error::Error;
use std::fmt;

/// Why a map cannot be walked as its mode needs, for the widest walker among the map's
/// structures. Waypoints and neutral spawns count from 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapProblem {
    /// A structure stands so near `team`'s avatar spawn that the widest walker cannot stand on it.
    SpawnBlocked { team: String },
    /// A structure stands so near neutral spawn `spawn` that the widest walker cannot stand on it.
    NeutralSpawnBlocked { spawn: usize },
    /// A structure stands so near `waypoint` of `path` that the widest walker cannot stand on it,
    /// and so never reaches it.
    WaypointBlocked { path: String, waypoint: usize },
    /// The structures close every way from the waypoint before `waypoint` of `path` to it.
    WaypointUnreachable { path: String, waypoint: usize },
}

impl fmt::Display for MapProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapProblem::SpawnBlocked { team } => write!(
                f,
                "a structure stands so near {team:?}'s avatar spawn that the widest unit that \
                 walks cannot stand on it"
            ),
            MapProblem::NeutralSpawnBlocked { spawn } => write!(
                f,
                "a structure stands so near neutral spawn {spawn} that the widest unit that walks \
                 cannot stand on it"
            ),
            MapProblem::WaypointBlocked { path, waypoint } => write!(
                f,
                "a structure stands so near waypoint {waypoint} of path {path:?} that the widest \
                 unit that walks cannot stand on it"
            ),
            MapProblem::WaypointUnreachable { path, waypoint } => write!(
                f,
                "the structures close every way to waypoint {waypoint} of path {path:?} from the \
                 one before it"
            ),
        }
    }
}

impl Error for MapProblem {}
