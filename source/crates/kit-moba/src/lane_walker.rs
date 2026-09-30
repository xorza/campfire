use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A creep walking a lane: while it has no attack target, it walks to the lane's waypoints in its
/// team's order, and stays at the last.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneWalker {
    lane: u32,
    /// The waypoint it walks to next, counted in its team's order.
    next: u32,
}

impl LaneWalker {
    pub(crate) const fn new(lane: u32, next: u32) -> LaneWalker {
        LaneWalker { lane, next }
    }

    pub const fn lane(self) -> u32 {
        self.lane
    }

    pub const fn next(self) -> u32 {
        self.next
    }

    pub(crate) const fn advance(&mut self) {
        self.next += 1;
    }
}

impl SimComponent for LaneWalker {
    const NAME: &'static str = "moba.lane_walker";
}
