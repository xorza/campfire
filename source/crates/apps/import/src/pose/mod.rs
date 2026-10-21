use bevy_math::Vec3;

use crate::quaternion::Quaternion;

/// A rigid placement: a rotation, then a translation, each step in `f32` in a fixed order, so
/// every machine gives the same bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RigidPose {
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quaternion,
}

impl RigidPose {
    pub(crate) const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quaternion::IDENTITY,
    };

    /// `child`, placed in this pose's frame.
    pub(crate) fn then(self, child: Self) -> Self {
        Self {
            translation: self.rotation.rotate(child.translation) + self.translation,
            rotation: self.rotation * child.rotation,
        }
    }

    pub(crate) fn place(self, point: Vec3) -> Vec3 {
        self.rotation.rotate(point) + self.translation
    }

    pub(crate) fn rotate(self, vector: Vec3) -> Vec3 {
        self.rotation.rotate(vector)
    }
}

#[cfg(test)]
mod tests;
