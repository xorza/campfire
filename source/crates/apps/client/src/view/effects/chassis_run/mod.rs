use std::f32::consts::FRAC_PI_2;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::math::{Quat, Vec3};
use campfire_client_files::Chassis;

use crate::view::float_num::FloatNum;

/// The chassis of the drawn tracked and wheeled units, each pitched and rolled on its springs
/// toward the ground under it and kicked by its drawing's change of velocity, each step of the
/// effects file, as Zero Hour's `Drawable::calcPhysicsXformTreads` moves a tank's every frame
/// (02-engine-core.md, A chassis follows the ground on springs).
#[derive(Debug, Default)]
pub(crate) struct ChassisRun {
    sprung: Vec<Sprung>,
}

/// On a drawing's root that its chassis tilts: the chassis turns it by its yaw and its tilt, and
/// its glide turns it none.
#[derive(Component, Debug)]
pub(crate) struct Tilts;

/// A type's chassis as the client steps it: the effects file's `Chassis` in floats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChassisPlay {
    pub(crate) pitch_stiffness: f32,
    pub(crate) roll_stiffness: f32,
    pub(crate) pitch_damping: f32,
    pub(crate) roll_damping: f32,
    pub(crate) axial_damping: f32,
    pub(crate) forward_pitch: f32,
    pub(crate) lateral_roll: f32,
    pub(crate) accelerating: f32,
    pub(crate) braking: f32,
}

/// A drawing as its chassis follows it: the yaw its unit faces at, in radians, the velocity it
/// moves at, in metres a second, and the normal of the ground under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ride {
    pub(crate) yaw: f32,
    pub(crate) velocity: Vec3,
    pub(crate) normal: Vec3,
}

/// A pitch or a roll and its rate, in radians and radians a step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Axis {
    angle: f32,
    rate: f32,
}

/// A pitch and a roll, in radians: a pitch down and a roll of its left side down positive, as
/// Zero Hour's `PhysicsXformInfo` holds them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Tilt {
    pub(crate) pitch: f32,
    pub(crate) roll: f32,
}

/// A drawing's chassis: its root, its type's springs, its pitch and its roll toward the ground,
/// the pitch and the roll its change of velocity kicks, the velocity it moved at the step before,
/// and its tilt at the last two steps.
#[derive(Debug, Clone, PartialEq)]
struct Sprung {
    root: Entity,
    play: ChassisPlay,
    pitch: Axis,
    roll: Axis,
    kick_pitch: Axis,
    kick_roll: Axis,
    velocity: Vec3,
    before: Tilt,
    now: Tilt,
}

impl ChassisRun {
    /// Gives the drawing `root`, moving at `velocity`, a chassis of `play`, level.
    pub(crate) fn add(&mut self, root: Entity, play: ChassisPlay, velocity: Vec3) {
        self.sprung.push(Sprung {
            root,
            play,
            pitch: Axis::default(),
            roll: Axis::default(),
            kick_pitch: Axis::default(),
            kick_roll: Axis::default(),
            velocity,
            before: Tilt::default(),
            now: Tilt::default(),
        });
    }

    /// Steps each chassis a step of `step` seconds, its drawing as `ride` gives it; a drawing
    /// `ride` gives none of went, and its chassis with it.
    pub(crate) fn step(&mut self, step: f32, ride: impl Fn(Entity) -> Option<Ride>) {
        self.sprung.retain_mut(|sprung| {
            let Some(ridden) = ride(sprung.root) else {
                return false;
            };
            sprung.step(step, ridden);
            true
        });
    }

    /// Each drawing's root and its chassis's turn, `fraction` of the way from its tilt at the
    /// step before the last to its tilt at the last, as Zero Hour draws between its frames.
    pub(crate) fn turns(&self, fraction: f32) -> impl Iterator<Item = (Entity, Quat)> + '_ {
        self.sprung.iter().map(move |sprung| {
            let tilt = Tilt {
                pitch: sprung.before.pitch + (sprung.now.pitch - sprung.before.pitch) * fraction,
                roll: sprung.before.roll + (sprung.now.roll - sprung.before.roll) * fraction,
            };
            (sprung.root, tilt.turn())
        })
    }
}

impl ChassisPlay {
    /// The client's play of `chassis`.
    pub(crate) fn of(chassis: &Chassis) -> Self {
        Self {
            pitch_stiffness: chassis.pitch_stiffness.float(),
            roll_stiffness: chassis.roll_stiffness.float(),
            pitch_damping: chassis.pitch_damping.float(),
            roll_damping: chassis.roll_damping.float(),
            axial_damping: chassis.axial_damping.float(),
            forward_pitch: chassis.forward_pitch.float(),
            lateral_roll: chassis.lateral_roll.float(),
            accelerating: chassis.accelerating.float(),
            braking: chassis.braking.float(),
        }
    }
}

impl Tilt {
    /// Its turn of a drawing whose model faces `x`, up `y`: pitched about its right, `z`, then
    /// rolled about its facing, as Zero Hour turns its matrix about its left and its facing.
    pub(crate) fn turn(self) -> Quat {
        Quat::from_rotation_z(-self.pitch) * Quat::from_rotation_x(-self.roll)
    }
}

impl Axis {
    /// Moves it by a spring of `stiffness` toward `rest` and a damper of `damping`, a step.
    fn spring(&mut self, rest: f32, stiffness: f32, damping: f32) {
        self.rate += -stiffness * (self.angle - rest) - damping * self.rate;
    }
}

impl Sprung {
    /// Steps it `step` seconds, its drawing as `ride` gives it: its pitch and its
    /// roll by their springs toward the ground's slope along and across its facing, times π/2, a
    /// pitch's rate up halved, scaled by its axes' damping; its kicks back toward none; then its
    /// tilt, the two together; then a kick of its change of velocity along and across its facing
    /// this step, and its kicks held within its limits.
    fn step(&mut self, step: f32, ride: Ride) {
        let play = self.play;
        let normal = ride.normal;
        let (sin, cos) = ride.yaw.sin_cos();
        let facing = Vec3::new(cos, 0.0, -sin);
        let left = Vec3::new(-sin, 0.0, -cos);
        let ground = Tilt {
            pitch: normal.dot(facing) * FRAC_PI_2,
            roll: normal.dot(left) * FRAC_PI_2,
        };
        self.pitch
            .spring(ground.pitch, play.pitch_stiffness, play.pitch_damping);
        if self.pitch.rate > 0.0 {
            self.pitch.rate *= 0.5;
        }
        self.roll
            .spring(ground.roll, play.roll_stiffness, play.roll_damping);
        self.pitch.angle += self.pitch.rate * play.axial_damping;
        self.roll.angle += self.roll.rate * play.axial_damping;
        self.kick_pitch
            .spring(0.0, play.pitch_stiffness, play.pitch_damping);
        self.kick_pitch.angle += self.kick_pitch.rate;
        self.kick_roll
            .spring(0.0, play.roll_stiffness, play.roll_damping);
        self.kick_roll.angle += self.kick_roll.rate;
        self.before = self.now;
        self.now = Tilt {
            pitch: self.pitch.angle + self.kick_pitch.angle,
            roll: self.roll.angle + self.kick_roll.angle,
        };
        // A step's change of velocity is its acceleration in metres a step squared.
        let change = (ride.velocity - self.velocity) * step;
        self.velocity = ride.velocity;
        self.kick_pitch.rate -= play.forward_pitch * change.dot(facing);
        self.kick_roll.rate -= play.lateral_roll * change.dot(left);
        // Down past its braking's limit, else up past its accelerating's, as Zero Hour tests them.
        let hold = |angle: f32| {
            if angle > play.braking {
                play.braking
            } else if angle < -play.accelerating {
                -play.accelerating
            } else {
                angle
            }
        };
        self.kick_pitch.angle = hold(self.kick_pitch.angle);
        self.kick_roll.angle = hold(self.kick_roll.angle);
    }
}

#[cfg(test)]
mod tests;
