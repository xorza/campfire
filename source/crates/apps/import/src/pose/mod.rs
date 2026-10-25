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

    /// The pose that undoes this one: its rotation back, then its move turned back and negated.
    pub(crate) fn inverse(self) -> Self {
        let rotation = self.rotation.conjugate();
        Self {
            translation: -rotation.rotate(self.translation),
            rotation,
        }
    }

    /// Its 4×4 matrix, its columns in order: the turned axes, then its move.
    pub(crate) fn matrix(self) -> [f32; 16] {
        let [x, y, z] = [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| self.rotation.rotate(axis));
        let t = self.translation;
        [
            x.x, x.y, x.z, 0.0, y.x, y.y, y.z, 0.0, z.x, z.y, z.z, 0.0, t.x, t.y, t.z, 1.0,
        ]
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
