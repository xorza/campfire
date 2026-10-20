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
mod tests {
    use std::f32::consts::FRAC_1_SQRT_2;

    use super::*;

    /// √½ is rounded in `f32`, so a quarter turn's result is within a few ulps of exact.
    const CLOSE: f32 = 1e-6;

    #[test]
    fn a_pose_turns_and_moves_as_wwmath_does() {
        // A quarter turn about z, (0, 0, √½, √½), takes x to y; then the move by (1, 2, 3).
        let half = FRAC_1_SQRT_2;
        let turn = RigidPose {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quaternion::from_array([0.0, 0.0, half, half]),
        };
        let placed = turn.place(Vec3::X);
        assert!(
            placed.abs_diff_eq(Vec3::new(1.0, 3.0, 3.0), CLOSE),
            "{placed}"
        );
        // Two quarter turns are a half turn: x goes to −x, and the child's move is turned too.
        let twice = turn.then(turn);
        let turned = twice.rotate(Vec3::X);
        assert!(turned.abs_diff_eq(Vec3::NEG_X, CLOSE), "{turned}");
        let moved = twice.translation;
        assert!(
            moved.abs_diff_eq(Vec3::new(-1.0, 3.0, 6.0), CLOSE),
            "{moved}"
        );
    }
}
