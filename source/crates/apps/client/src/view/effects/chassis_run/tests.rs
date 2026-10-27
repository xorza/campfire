use std::f32::consts::PI;

use super::*;

/// A step of 1/32 s, exact in binary.
const STEP: f32 = 1.0 / 32.0;

/// Springs of a pitch's stiffness 0.5 and a roll's 0.25, each damped by half, axes kept whole, no
/// kick, and limits of 0.05 rad.
const PLAY: ChassisPlay = ChassisPlay {
    pitch_stiffness: 0.5,
    roll_stiffness: 0.25,
    pitch_damping: 0.5,
    roll_damping: 0.5,
    axial_damping: 1.0,
    forward_pitch: 0.0,
    lateral_roll: 0.0,
    accelerating: 0.05,
    braking: 0.05,
};

/// The tilts at the steps of a chassis of `play`, facing `x`, as `velocities` and `normal` carry
/// it, one step for each velocity, from a start at none.
fn tilts(play: ChassisPlay, normal: Vec3, velocities: &[Vec3]) -> Vec<Tilt> {
    let root = Entity::from_raw_u32(1).unwrap();
    let mut run = ChassisRun::default();
    run.add(root, play, Vec3::ZERO);
    velocities
        .iter()
        .map(|&velocity| {
            run.step(STEP, |_| {
                Some(Ride {
                    yaw: 0.0,
                    velocity,
                    normal,
                })
            });
            run.sprung[0].now
        })
        .collect()
}

fn close(tilts: &[Tilt], pitches: &[f32], rolls: &[f32]) {
    for ((tilt, pitch), roll) in tilts.iter().zip(pitches).zip(rolls) {
        assert!((tilt.pitch - pitch).abs() < 1e-6, "{tilts:?}");
        assert!((tilt.roll - roll).abs() < 1e-6, "{tilts:?}");
    }
}

#[test]
fn a_chassis_settles_on_its_springs_toward_the_slope_under_it() {
    let still = [Vec3::ZERO; 3];
    // Uphill ahead, a normal of −0.2 along its facing: a ground pitch of −0.2 × π/2 = −0.1π, nose
    // up. The rate is 0.5 × 0.1π = 0.05π down the first step, −0.05π; then −0.05π − 0.5 × 0.05π +
    // 0.5 × 0.05π, the same, −0.1π; then −0.05π + 0.025π, −0.125π, past the ground's, a spring's.
    let up = tilts(PLAY, Vec3::new(-0.2, 1.0, 0.0), &still);
    close(&up, &[-0.05 * PI, -0.1 * PI, -0.125 * PI], &[0.0; 3]);
    // Downhill, a rate down is halved: 0.05π halved, 0.025π.
    let down = tilts(PLAY, Vec3::new(0.2, 1.0, 0.0), &still[..1]);
    close(&down, &[0.025 * PI], &[0.0]);
    // Its left down, a normal of 0.2 along its left, −z: a ground roll of 0.1π, at the roll's 0.25,
    // 0.025π, with no halving.
    let left = tilts(PLAY, Vec3::new(0.0, 1.0, -0.2), &still[..1]);
    close(&left, &[0.0], &[0.025 * PI]);
    // Axes kept half, the pitch moves half its rate: −0.025π.
    let halved = ChassisPlay {
        axial_damping: 0.5,
        ..PLAY
    };
    close(
        &tilts(halved, Vec3::new(-0.2, 1.0, 0.0), &still[..1]),
        &[-0.025 * PI],
        &[0.0],
    );
}

#[test]
fn a_chassis_rocks_as_it_accelerates_held_within_its_limits() {
    // From rest to 4 m/s along its facing in a step, a change of 4 × 1/32 = 0.125 a step: by a
    // factor of 1, a kick of −0.125 to the pitch's rate after the first step's tilt, 0. The next,
    // its spring and damper take −0.125 to −0.0625, the tilt −0.0625, then the kick is held at the
    // accelerating limit, −0.05; the next, −0.0625 + 0.025 + 0.03125 = −0.00625, a tilt of
    // −0.05625, held at −0.05 again.
    let rocking = ChassisPlay {
        forward_pitch: 1.0,
        ..PLAY
    };
    let ahead = Vec3::new(4.0, 0.0, 0.0);
    let pitched = tilts(rocking, Vec3::Y, &[ahead; 3]);
    close(&pitched, &[0.0, -0.0625, -0.05625], &[0.0; 3]);
    // Across, to 4 m/s along its left, −z, by a roll's factor of 0.4: a kick of −0.05 to the roll's
    // rate, at the roll's springs, −0.05 + 0.25 × 0 + 0.5 × 0.05 = −0.025.
    let leaning = ChassisPlay {
        lateral_roll: 0.4,
        ..PLAY
    };
    let rolled = tilts(leaning, Vec3::Y, &[Vec3::new(0.0, 0.0, -4.0); 2]);
    close(&rolled, &[0.0, 0.0], &[0.0, -0.025]);
}

#[test]
fn a_tilt_pitches_its_nose_and_rolls_its_left_down_and_draws_between_its_steps() {
    // A pitch of 0.1 takes its facing, x, down; a roll of 0.1 its left, −z, down.
    let nose = Tilt {
        pitch: 0.1,
        roll: 0.0,
    }
    .turn()
        * Vec3::X;
    assert!((nose - Vec3::new(0.1_f32.cos(), -0.1_f32.sin(), 0.0)).length() < 1e-6);
    let side = Tilt {
        pitch: 0.0,
        roll: 0.1,
    }
    .turn()
        * Vec3::NEG_Z;
    assert!((side - Vec3::new(0.0, -0.1_f32.sin(), -0.1_f32.cos())).length() < 1e-6);
    // A quarter of the way from the first step's tilt, −0.05π, to the second's, −0.1π.
    let root = Entity::from_raw_u32(1).unwrap();
    let mut run = ChassisRun::default();
    run.add(root, PLAY, Vec3::ZERO);
    let carried = |_| {
        Some(Ride {
            yaw: 0.0,
            velocity: Vec3::ZERO,
            normal: Vec3::new(-0.2, 1.0, 0.0),
        })
    };
    run.step(STEP, carried);
    run.step(STEP, carried);
    let turns: Vec<(Entity, Quat)> = run.turns(0.25).collect();
    let quarter = Tilt {
        pitch: -0.0625 * PI,
        roll: 0.0,
    };
    assert_eq!(turns.len(), 1);
    assert!(turns[0].1.abs_diff_eq(quarter.turn(), 1e-6), "{turns:?}");
    // A drawing gone takes its chassis with it.
    run.step(STEP, |_| None);
    assert_eq!(run.turns(0.0).count(), 0);
}
