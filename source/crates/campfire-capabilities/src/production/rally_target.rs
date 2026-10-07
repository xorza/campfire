use campfire_math::Num;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// Where a producer sends the units it trains: a point of the ground plane, or a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RallyTarget {
    Point { x: Num, z: Num },
    Unit(StableId),
}
