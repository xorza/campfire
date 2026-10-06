use thiserror::Error;

use crate::values::declared_name::DeclaredName;

/// Why a map cannot be walked as its mode needs, for the widest walker among the map's walls and
/// placed units that cannot walk. Waypoints count from 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapProblem {
    /// A wall or a placed unit stands so near the point of `marker` that the widest walker cannot
    /// stand on it.
    #[error(
        "a wall or a placed unit stands so near marker \"{marker}\" that the widest unit \
                 that walks cannot stand on it"
    )]
    MarkerBlocked { marker: DeclaredName },
    /// A wall or a structure stands so near `waypoint` of `path` that the widest walker cannot
    /// stand on it, and so never reaches it.
    #[error(
        "a wall or a placed unit stands so near waypoint {waypoint} of path \"{path}\" that \
                 the widest unit that walks cannot stand on it"
    )]
    WaypointBlocked { path: DeclaredName, waypoint: usize },
    /// The walls and structures close every way from the waypoint before `waypoint` of `path` to
    /// it.
    #[error(
        "the walls and placed units close every way to waypoint {waypoint} of path \"{path}\" \
                 from the one before it"
    )]
    WaypointUnreachable { path: DeclaredName, waypoint: usize },
}
