use std::error::Error;
use std::fmt;

use crate::values::declared_name::DeclaredName;

/// Why a map cannot be walked as its mode needs, for the widest walker among the map's placed
/// units that cannot walk. Waypoints count from 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapProblem {
    /// A placed unit stands so near the point of `marker` that the widest walker cannot stand on
    /// it.
    MarkerBlocked { marker: DeclaredName },
    /// A structure stands so near `waypoint` of `path` that the widest walker cannot stand on it,
    /// and so never reaches it.
    WaypointBlocked { path: DeclaredName, waypoint: usize },
    /// The structures close every way from the waypoint before `waypoint` of `path` to it.
    WaypointUnreachable { path: DeclaredName, waypoint: usize },
}

impl fmt::Display for MapProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapProblem::MarkerBlocked { marker } => write!(
                f,
                "a placed unit stands so near marker {marker:?} that the widest unit that walks \
                 cannot stand on it"
            ),
            MapProblem::WaypointBlocked { path, waypoint } => write!(
                f,
                "a placed unit stands so near waypoint {waypoint} of path {path:?} that the widest \
                 unit that walks cannot stand on it"
            ),
            MapProblem::WaypointUnreachable { path, waypoint } => write!(
                f,
                "the placed units close every way to waypoint {waypoint} of path {path:?} from the \
                 one before it"
            ),
        }
    }
}

impl Error for MapProblem {}
