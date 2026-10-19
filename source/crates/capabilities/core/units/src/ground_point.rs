use campfire_math::{Num, Vec3};
use serde::{Deserialize, Serialize};

/// A point of the ground plane, in meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundPoint {
    pub x: Num,
    pub z: Num,
}

impl GroundPoint {
    pub const fn new(x: Num, z: Num) -> Self {
        Self { x, z }
    }

    /// The point of the ground plane under `pos`.
    pub const fn under(pos: Vec3) -> Self {
        Self { x: pos.x, z: pos.z }
    }
}
