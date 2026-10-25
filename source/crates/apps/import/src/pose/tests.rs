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
    // A half turn about z at (1, 2, 3), exact in `f32`: its matrix's columns are the turned axes,
    // −x, −y and z, then its move. Its inverse turns back and moves by −(−1, −2, 3) = (1, 2, −3),
    // so the pose then its inverse is the identity.
    let half = RigidPose {
        translation: Vec3::new(1.0, 2.0, 3.0),
        rotation: Quaternion::from_array([0.0, 0.0, 1.0, 0.0]),
    };
    assert_eq!(
        half.matrix(),
        [
            -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0
        ]
    );
    let inverse = half.inverse();
    assert_eq!(inverse.translation, Vec3::new(1.0, 2.0, -3.0));
    let undone = half.then(inverse);
    assert_eq!(undone.translation, Vec3::ZERO);
    assert_eq!(undone.rotation.to_array(), [0.0, 0.0, 0.0, 1.0]);
}
