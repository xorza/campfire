//! Interpolation fns for common types, which a rule can name as its lerp.
use bevy_curve::{Curve, Ease, EaseFunction, EasingCurve};
use bevy_transform::components::Transform;

/// The linear interpolation from `start` to `other` along their [`Ease`] curve.
pub fn linear_lerp<C>(start: &C, other: &C, t: f32) -> C
where
    C: Ease + Clone,
{
    let curve = EasingCurve::new(start.clone(), other.clone(), EaseFunction::Linear);
    curve.sample_unchecked(t)
}

/// The linear interpolation of a [`Transform`]: its translation and scale lerp, and its rotation
/// slerps.
pub fn transform_lerp(start: &Transform, other: &Transform, t: f32) -> Transform {
    Transform {
        translation: start.translation.lerp(other.translation, t),
        rotation: start.rotation.slerp(other.rotation, t),
        scale: start.scale.lerp(other.scale, t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{Quat, Vec3};
    use core::f32::consts::FRAC_PI_2;
    use core::f32::consts::FRAC_PI_4;

    /// Halfway: translation (0, 0, 0) to (2, 4, 6) is (1, 2, 3), scale 1 to 3 is 2, and a quarter
    /// turn about z slerps to an eighth.
    #[test]
    fn transform_lerps_and_slerps() {
        let start = Transform::IDENTITY;
        let end = Transform {
            translation: Vec3::new(2.0, 4.0, 6.0),
            rotation: Quat::from_rotation_z(FRAC_PI_2),
            scale: Vec3::splat(3.0),
        };
        let half = transform_lerp(&start, &end, 0.5);
        assert_eq!(half.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(half.scale, Vec3::splat(2.0));
        // The slerp's trigonometry rounds in the last bits of an f32.
        assert!(
            half.rotation
                .abs_diff_eq(Quat::from_rotation_z(FRAC_PI_4), 1e-6)
        );
    }
}
