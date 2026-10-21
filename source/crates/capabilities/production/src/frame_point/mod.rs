use campfire_math::{Num, SinCos, Vec3};
use serde::{Deserialize, Serialize, Serializer};

use crate::error::PointPastBound;

/// A point in a producer's own frame, `[x, z]` in meters from its position, each within
/// `Vec3::BOUND`: where its trained units come out, or go with no rally point set. It turns
/// with the producer as its box body turns by its angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "[Num; 2]")]
pub struct FramePoint([Num; 2]);

impl FramePoint {
    /// `[x, z]`; `None` for a coordinate past `Vec3::BOUND`.
    pub fn new(point: [Num; 2]) -> Option<Self> {
        let within = |at: &Num| at.to_bits().unsigned_abs() <= Vec3::BOUND.to_bits().unsigned_abs();
        point.iter().all(within).then_some(Self(point))
    }

    /// The point of a producer at `position` that faces `angle` degrees, at its height: the frame's
    /// `x` turned as its box's first half edge turns and its `z` as its second, from `x`
    /// towards `−z`, each coordinate rounded once.
    pub fn placed(self, position: Vec3, angle: Num) -> Vec3 {
        let [x, z] = self.0;
        let turned = Vec3::new(x, Num::ZERO, z).rotated_y(SinCos::of_degrees(angle));
        Vec3::new(position.x + turned.x, position.y, position.z + turned.z)
    }
}

/// As `[x, z]`, which `Deserialize` reads.
impl Serialize for FramePoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl TryFrom<[Num; 2]> for FramePoint {
    type Error = PointPastBound;

    fn try_from(point: [Num; 2]) -> Result<Self, PointPastBound> {
        Self::new(point).ok_or(PointPastBound(point))
    }
}

#[cfg(test)]
mod tests;
