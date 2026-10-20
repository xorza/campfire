use std::ops::Mul;

use bevy_math::Vec3;

/// A rotation, the unit quaternion `x, y, z, w` of the standard matrix, as WW3D's `Build_Matrix3`
/// and a glTF node turn by it. Not `glam`'s `Quat`, whose product fuses its multiplies and adds on
/// ARM's NEON and not on x86's SSE2: each step here is `f32`, in a fixed order with no fused
/// operation, so every machine gives the same bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Quaternion {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

impl Quaternion {
    pub(crate) const IDENTITY: Self = Self::from_array([0.0, 0.0, 0.0, 1.0]);

    /// The quaternion `x, y, z, w`.
    pub(crate) const fn from_array([x, y, z, w]: [f32; 4]) -> Self {
        Self { x, y, z, w }
    }

    pub(crate) const fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }

    /// `vector` turned: `v + 2w (q × v) + 2 q × (q × v)`.
    pub(crate) fn rotate(self, vector: Vec3) -> Vec3 {
        let axis = Vec3::new(self.x, self.y, self.z);
        let once = axis.cross(vector);
        let twice = axis.cross(once);
        vector + 2.0 * (self.w * once) + 2.0 * twice
    }

    /// The row of the rotation's matrix that gives a turned vector's height.
    pub(crate) fn up_row(self) -> Vec3 {
        let Self { x, y, z, w } = self;
        Vec3::new(
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        )
    }
}

/// The rotation `rhs`, then this one.
impl Mul for Quaternion {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let Self {
            x: ax,
            y: ay,
            z: az,
            w: aw,
        } = self;
        let Self {
            x: bx,
            y: by,
            z: bz,
            w: bw,
        } = rhs;
        Self {
            x: aw * bx + ax * bw + ay * bz - az * by,
            y: aw * by - ax * bz + ay * bw + az * bx,
            z: aw * bz + ax * by - ay * bx + az * bw,
            w: aw * bw - ax * bx - ay * by - az * bz,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_1_SQRT_2;

    use super::*;

    /// √½ is rounded in `f32`, so a quarter turn's result is within a few ulps of exact.
    const CLOSE: f32 = 1e-6;

    /// A quarter turn about `z` takes `x` to `y`, and two of them `x` to `−x`; the row of the
    /// height of a quarter turn about `x`, which takes `z` to `−y`, is `(0, 0, −1)`.
    #[test]
    fn a_quaternion_turns_as_wwmath_does() {
        let half = FRAC_1_SQRT_2;
        let about_z = Quaternion::from_array([0.0, 0.0, half, half]);
        let turned = about_z.rotate(Vec3::X);
        assert!(turned.abs_diff_eq(Vec3::Y, CLOSE), "{turned}");
        let twice = (about_z * about_z).rotate(Vec3::X);
        assert!(twice.abs_diff_eq(Vec3::NEG_X, CLOSE), "{twice}");
        let about_x = Quaternion::from_array([half, 0.0, 0.0, half]);
        assert!(about_x.up_row().abs_diff_eq(Vec3::NEG_Z, CLOSE));
        assert_eq!(Quaternion::IDENTITY.rotate(Vec3::ONE), Vec3::ONE);
        assert_eq!(Quaternion::IDENTITY.up_row(), Vec3::Y);
    }
}
