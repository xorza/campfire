use campfire_math::Num;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// Where a build order places its building: at a point of the ground plane, its box turned
/// `angle` degrees, or at a site of the same building to build on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildTarget {
    Point { x: Num, z: Num, angle: Num },
    Site(StableId),
}
