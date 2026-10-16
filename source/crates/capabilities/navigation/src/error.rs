use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_values::DeclaredName;
use thiserror::Error;

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
    /// Placed unit `unit`, of `unit_type`, a box, reaches past the bounds, or overlaps a wall of
    /// its layer or another placed unit of its layer that cannot walk.
    #[error(
        "placed unit {unit}, a \"{unit_type}\" box, reaches past the bounds or overlaps a wall \
                 or another placed unit that cannot walk"
    )]
    BoxBlocked {
        unit: usize,
        unit_type: DeclaredName,
    },
}

campfire_common::enum_all! {
    /// Why the navigation capability refused a script call: a script gave it a value it does not
    /// take.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NavigationRefusal {
        /// A distance that is not more than 0, or beyond the world's bound.
        NotADistance,
        /// A speed of no step a tick: 0, negative, or too small to move a bit.
        NotASpeed,
        /// A forced move of a unit that does not walk.
        NoWalker,
    }
    const ALL;
}

impl CapabilityRefusal for NavigationRefusal {
    const OWNER: Capability = Capability::Navigation;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NotADistance => "distance is not more than 0, or beyond the world's bound",
            Self::NotASpeed => "speed moves less than a bit a tick",
            Self::NoWalker => "the unit does not walk",
        }
    }
}
