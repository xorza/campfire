use crate::diffable::Diffable;
use bevy_math::{Quat, Rot2, Vec2, Vec3};
use bevy_transform::prelude::Transform;

/// The change from one 2D [`Transform`] to another, which prediction's correction applies: a
/// translation, a rotation about z and a scale in the plane. It is exact for a transform that
/// rotates about z only and keeps its z scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformDiff2d {
    /// The translation, z included, as a 2D transform's z orders its layers.
    pub translation: Vec3,
    pub rotation: Rot2,
    pub scale: Vec2,
}

impl Diffable<TransformDiff2d> for Transform {
    fn base_value() -> Self {
        Self::default()
    }

    fn diff(&self, new: &Self) -> TransformDiff2d {
        // The angle about z of the relative quaternion, read from the z and w components.
        let (z1, w1) = (self.rotation.z, self.rotation.w);
        let (z2, w2) = (new.rotation.z, new.rotation.w);
        let rotation = 2.0 * (z2 * w1 - w2 * z1).atan2(w2 * w1 + z2 * z1);
        TransformDiff2d {
            translation: new.translation - self.translation,
            rotation: Rot2::radians(rotation),
            scale: new.scale.truncate() - self.scale.truncate(),
        }
    }

    fn apply_diff(&mut self, delta: &TransformDiff2d) {
        self.translation += delta.translation;
        self.rotation *= Quat::from_rotation_z(delta.rotation.as_radians());
        self.scale += delta.scale.extend(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applying the diff to its base gives the new 2D transform, its z layer and scale
    /// included. The rotation goes through an angle and a quaternion, whose rounding the 1e-6
    /// tolerance covers.
    #[test]
    fn a_diff_applies_back_to_the_new_transform() {
        let base = Transform::from_xyz(1.0, 2.0, 0.0)
            .with_rotation(Quat::from_rotation_z(0.5))
            .with_scale(Vec3::new(1.0, 2.0, 1.0));
        let new = Transform::from_xyz(4.0, -2.0, 3.0)
            .with_rotation(Quat::from_rotation_z(-1.25))
            .with_scale(Vec3::new(3.0, 0.5, 1.0));

        let mut applied = base;
        let diff: TransformDiff2d = base.diff(&new);
        applied.apply_diff(&diff);

        assert_eq!(applied.translation, new.translation);
        assert_eq!(applied.scale, new.scale);
        assert!(applied.rotation.abs_diff_eq(new.rotation, 1e-6));
    }
}
