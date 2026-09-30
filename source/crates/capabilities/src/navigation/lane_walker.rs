use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A unit walking the lane its `OnLane` names, such as a creep: it walks to the lane's waypoints
/// in its direction while it has no other order, and stays at the last.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneWalker {
    direction: PathDirection,
    /// The waypoint it walks to next, counted in its direction.
    next: u32,
}

/// Which way a unit walks a path: a MOBA's two sides walk each lane from their own end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathDirection {
    Forward,
    Backward,
}

impl LaneWalker {
    /// A walker at the start of its lane in `direction`, bound for its second waypoint.
    pub const fn start(direction: PathDirection) -> LaneWalker {
        LaneWalker { direction, next: 1 }
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

impl SimComponent for LaneWalker {
    const NAME: &'static str = "navigation.lane_walker";
}
