use crate::diffable::Diffable;
use bevy_math::{Quat, Vec3};
use bevy_transform::prelude::Transform;

/// The change from one [`Transform`] to another, which prediction's correction applies: a
/// translation, a rotation and a scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformDiff {
    pub translation: Vec3,
    /// The rotation that follows the base's: the new one is `base * rotation`.
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Diffable<TransformDiff> for Transform {
    fn base_value() -> Self {
        Self::default()
    }

    fn diff(&self, new: &Self) -> TransformDiff {
        TransformDiff {
            translation: new.translation - self.translation,
            rotation: self.rotation.inverse() * new.rotation,
            scale: new.scale - self.scale,
        }
    }

    fn apply_diff(&mut self, delta: &TransformDiff) {
        self.translation += delta.translation;
        self.rotation *= delta.rotation;
        self.scale += delta.scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applying the diff to its base gives the new transform, its scale included. The rotation
    /// goes through a quaternion product, whose rounding the 1e-6 tolerance covers.
    #[test]
    fn a_diff_applies_back_to_the_new_transform() {
        let base = Transform::from_xyz(1.0, 2.0, 3.0)
            .with_rotation(Quat::from_rotation_y(0.5))
            .with_scale(Vec3::new(1.0, 2.0, 1.0));
        let new = Transform::from_xyz(4.0, -2.0, 0.5)
            .with_rotation(Quat::from_rotation_x(1.25))
            .with_scale(Vec3::new(3.0, 0.5, 2.0));

        let mut applied = base;
        let diff: TransformDiff = base.diff(&new);
        applied.apply_diff(&diff);

        assert_eq!(applied.translation, new.translation);
        assert_eq!(applied.scale, new.scale);
        assert!(applied.rotation.abs_diff_eq(new.rotation, 1e-6));
    }
}
