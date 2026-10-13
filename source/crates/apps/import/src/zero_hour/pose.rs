use crate::zero_hour::hierarchy::Pivot;

/// A rigid placement in W3D's frame, `z` up: a rotation, a quaternion `x, y, z, w` with WW3D's
/// `Build_Matrix3`, the standard one, then a translation. Each step is `f32`, in a fixed order with
/// no fused operation, so every machine gives the same bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pose {
    pub(crate) translation: [f32; 3],
    pub(crate) rotation: [f32; 4],
}

impl Pose {
    pub(crate) const IDENTITY: Pose = Pose {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    /// A pivot's placement in its parent's frame.
    pub(crate) const fn of(pivot: &Pivot) -> Pose {
        Pose {
            translation: pivot.translation,
            rotation: pivot.rotation,
        }
    }

    /// The placement of each pivot in the model's frame: its parent's, then its own.
    pub(crate) fn of_each(pivots: &[Pivot]) -> Vec<Pose> {
        let mut poses: Vec<Pose> = Vec::with_capacity(pivots.len());
        for pivot in pivots {
            let local = Pose::of(pivot);
            let pose = match pivot.parent {
                Some(parent) => poses[parent].then(local),
                None => local,
            };
            poses.push(pose);
        }
        poses
    }

    /// `child`, placed in this pose's frame.
    pub(crate) fn then(self, child: Pose) -> Pose {
        Pose {
            translation: Pose::add(self.rotate(child.translation), self.translation),
            rotation: Pose::multiply(self.rotation, child.rotation),
        }
    }

    pub(crate) fn place(self, point: [f32; 3]) -> [f32; 3] {
        Pose::add(self.rotate(point), self.translation)
    }

    /// `vector` turned by the rotation: `v + 2w (q × v) + 2 q × (q × v)`.
    pub(crate) fn rotate(self, vector: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = self.rotation;
        let axis = [x, y, z];
        let once = Pose::cross(axis, vector);
        let twice = Pose::cross(axis, once);
        let mut out = [0.0; 3];
        for at in 0..3 {
            out[at] = vector[at] + 2.0 * (w * once[at]) + 2.0 * twice[at];
        }
        out
    }

    /// A point or a direction of W3D's frame on glTF's axes, `y` up: `(x, y, z)` as `(x, z, −y)`,
    /// a rotation, so handedness and winding hold.
    pub(crate) const fn y_up(vector: [f32; 3]) -> [f32; 3] {
        [vector[0], vector[2], -vector[1]]
    }

    /// A rotation of W3D's frame on glTF's axes: its axis turned as a vector is, its angle kept.
    pub(crate) const fn y_up_rotation(rotation: [f32; 4]) -> [f32; 4] {
        [rotation[0], rotation[2], -rotation[1], rotation[3]]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::f32::consts::FRAC_1_SQRT_2;

    /// √½ is rounded in `f32`, so a quarter turn's result is within a few ulps of exact.
    const CLOSE: f32 = 1e-6;

    #[test]
    fn a_pose_turns_and_moves_as_wwmath_does_and_turns_to_y_up() {
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
        // W3D's up, z, is glTF's y; its north, y, is glTF's −z.
        assert_eq!(Pose::y_up([1.0, 2.0, 3.0]), [1.0, 3.0, -2.0]);
        assert_eq!(
            Pose::y_up_rotation([0.1, 0.2, 0.3, 0.9]),
            [0.1, 0.3, -0.2, 0.9]
        );
    }
}
