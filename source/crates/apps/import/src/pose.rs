/// A rigid placement: a rotation, a quaternion `x, y, z, w` of the standard matrix, as WW3D's
/// `Build_Matrix3` and a glTF node turn by it, then a translation. Each step is `f32`, in a fixed
/// order with no fused operation, so every machine gives the same bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pose {
    pub(crate) translation: [f32; 3],
    pub(crate) rotation: [f32; 4],
}

impl Pose {
    pub(crate) const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    /// `child`, placed in this pose's frame.
    pub(crate) fn then(self, child: Self) -> Self {
        Self {
            translation: add(self.rotate(child.translation), self.translation),
            rotation: multiply(self.rotation, child.rotation),
        }
    }

    pub(crate) fn place(self, point: [f32; 3]) -> [f32; 3] {
        add(self.rotate(point), self.translation)
    }

    /// `vector` turned by the rotation: `v + 2w (q × v) + 2 q × (q × v)`.
    pub(crate) fn rotate(self, vector: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = self.rotation;
        let axis = [x, y, z];
        let once = cross(axis, vector);
        let twice = cross(axis, once);
        let mut out = [0.0; 3];
        for at in 0..3 {
            out[at] = vector[at] + 2.0 * (w * once[at]) + 2.0 * twice[at];
        }
        out
    }
}

fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
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
        let turn = Pose {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, half, half],
        };
        let [x, y, z] = turn.place([1.0, 0.0, 0.0]);
        assert!(
            (x - 1.0).abs() < CLOSE && (y - 3.0).abs() < CLOSE && (z - 3.0).abs() < CLOSE,
            "{x} {y} {z}"
        );
        // Two quarter turns are a half turn: x goes to −x.
        let twice = turn.then(Pose {
            translation: [0.0; 3],
            ..turn
        });
        let [x, y, _] = twice.rotate([1.0, 0.0, 0.0]);
        assert!((x + 1.0).abs() < CLOSE && y.abs() < CLOSE, "{x} {y}");
    }
}
