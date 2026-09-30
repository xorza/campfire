use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit walking the path its `OnPath` names, such as a creep: it walks to the path's waypoints
/// in its direction while it has no other order, and stays at the last.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathWalker {
    direction: PathDirection,
    /// The waypoint it walks to next, counted in its direction.
    next: u32,
}

/// Which way a unit walks a path: a MOBA's two sides walk each path from their own end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathDirection {
    Forward,
    Backward,
}

impl PathWalker {
    /// A walker at the start of its path in `direction`, bound for its second waypoint.
    pub const fn start(direction: PathDirection) -> PathWalker {
        PathWalker { direction, next: 1 }
    }

    pub const fn direction(self) -> PathDirection {
        self.direction
    }

    pub const fn next(self) -> u32 {
        self.next
    }

    pub(crate) const fn advance(&mut self) {
        self.next += 1;
    }
}

impl SimComponent for PathWalker {
    const NAME: &'static str = "navigation.path_walker";
}
