use bevy_math::Vec3;
use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::material_extras::Blend;
use crate::spread::Spread;

/// A particle system: what it emits and how its particles move, grow, fade and draw. Its times
/// count the effects file's steps, and its rates are a step's; its distances are world units on
/// the engine's axes, `y` up; its angles radians; its alphas and colors from 0 to 1.
///
/// Each step it lives, past its `initial_delay`, it emits `burst_count` particles every
/// `burst_delay` steps, for `system_lifetime` steps, or for ever at 0;
/// each particle starts in its `volume`, on its surface alone when `hollow`, and above the ground
/// alone when `above_ground_only`, at its `velocity`, and lives `lifetime` steps. A particle's
/// velocity is damped by its `velocity_damping` and pulled by `gravity` each step, and it drifts
/// by `drift`; its size starts at `size`, grows by its rate, which starts at `size_rate` plus
/// `start_size_rate` and is damped by `size_rate_damping`; it turns from `angle` by its
/// `angular_rate`, damped by `angular_damping`, or keeps its top towards the system when
/// `up_towards_emitter`; its alpha and its color run between their keys, and its color moves by
/// `color_scale` each step. It draws as a square of its texture facing the camera, or lying on
/// the ground when `ground_aligned`, in its `blend`. Each particle carries a system of
/// `attached`, and the system starts its `slave` beside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleSystem {
    /// Its rank under the file's cap, the lowest's particles removed first; none for a system
    /// that passes the cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u8>,
    pub blend: Blend,
    pub texture: PackagePath,
    pub ground_aligned: bool,
    pub up_towards_emitter: bool,
    pub above_ground_only: bool,
    pub angle: Spread,
    pub angular_rate: Spread,
    pub angular_damping: Spread,
    pub velocity_damping: Spread,
    pub gravity: f32,
    pub drift: Vec3,
    pub lifetime: Spread,
    pub system_lifetime: u32,
    pub size: Spread,
    pub start_size_rate: Spread,
    pub size_rate: Spread,
    pub size_rate_damping: Spread,
    /// Its alpha at its first key's step, then between each key and the next, at least one, each
    /// at a later step than the one before; past the last, the last's.
    pub alpha: Vec<AlphaKey>,
    /// Its color as its alpha runs.
    pub color: Vec<ColorKey>,
    pub color_scale: Spread,
    pub burst_delay: Spread,
    pub burst_count: Spread,
    pub initial_delay: Spread,
    pub velocity: EmissionVelocity,
    pub volume: EmissionVolume,
    pub hollow: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slave: Option<SlaveSystem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached: Option<DeclaredName>,
}

/// A particle's alpha at the step `at` of its life.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlphaKey {
    pub alpha: Spread,
    pub at: u32,
}

/// A particle's color, red, green and blue, at the step `at` of its life.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorKey {
    pub color: [f32; 3],
    pub at: u32,
}

/// A system a particle system starts beside it, `offset` from its place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlaveSystem {
    pub system: DeclaredName,
    pub offset: Vec3,
}

/// The velocity a particle starts at: none; a step's distance along each axis of the system; a
/// speed in a direction drawn on the sphere, or on its upper half; a speed out from the
/// cylinder's axis and one along it; or a speed out from the system's place through the particle's
/// start, and `other` along the system's axis for a particle of a line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmissionVelocity {
    Still,
    Ortho { x: Spread, y: Spread, z: Spread },
    Spherical { speed: Spread },
    Hemispherical { speed: Spread },
    Cylindrical { radial: Spread, normal: Spread },
    Outward { speed: Spread, other: Spread },
}

/// Where a particle starts, about the system's place: at it; on the line from `start` to `end`;
/// in a box of half the size `half`; in a sphere; or in an upright cylinder.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmissionVolume {
    Point,
    Line { start: Vec3, end: Vec3 },
    Box { half: Vec3 },
    Sphere { radius: f32 },
    Cylinder { radius: f32, length: f32 },
}

impl ParticleSystem {
    /// Whether each of its numbers is finite, and its keys each later than the one before, at
    /// least one of each.
    pub(crate) fn holds(&self) -> bool {
        let spreads = [
            self.angle,
            self.angular_rate,
            self.angular_damping,
            self.velocity_damping,
            self.lifetime,
            self.size,
            self.start_size_rate,
            self.size_rate,
            self.size_rate_damping,
            self.color_scale,
            self.burst_delay,
            self.burst_count,
            self.initial_delay,
        ];
        let alpha = self.alpha.iter().all(|key| key.alpha.is_finite());
        let color = self.color.iter().flat_map(|key| key.color);
        let alpha_steps = self.alpha.iter().map(|key| key.at);
        let color_steps = self.color.iter().map(|key| key.at);
        spreads.iter().all(|spread| spread.is_finite())
            && self.gravity.is_finite()
            && self.drift.is_finite()
            && alpha
            && color.into_iter().all(f32::is_finite)
            && Self::rising(alpha_steps)
            && Self::rising(color_steps)
            && self.velocity.is_finite()
            && self.volume.is_finite()
            && self
                .slave
                .as_ref()
                .is_none_or(|slave| slave.offset.is_finite())
    }

    /// Whether `steps` hold at least one, each later than the one before.
    fn rising(steps: impl Iterator<Item = u32>) -> bool {
        let mut last = None;
        for step in steps {
            if last.is_some_and(|last| step <= last) {
                return false;
            }
            last = Some(step);
        }
        last.is_some()
    }

    /// The systems it names, its slave's and its particles'.
    pub(crate) fn names(&self) -> impl Iterator<Item = &DeclaredName> {
        let slave = self.slave.as_ref().map(|slave| &slave.system);
        slave.into_iter().chain(&self.attached)
    }
}

impl EmissionVelocity {
    const fn is_finite(self) -> bool {
        match self {
            Self::Still => true,
            Self::Ortho { x, y, z } => x.is_finite() && y.is_finite() && z.is_finite(),
            Self::Spherical { speed } | Self::Hemispherical { speed } => speed.is_finite(),
            Self::Cylindrical { radial, normal } => radial.is_finite() && normal.is_finite(),
            Self::Outward { speed, other } => speed.is_finite() && other.is_finite(),
        }
    }
}

impl EmissionVolume {
    fn is_finite(self) -> bool {
        match self {
            Self::Point => true,
            Self::Line { start, end } => start.is_finite() && end.is_finite(),
            Self::Box { half } => half.is_finite(),
            Self::Sphere { radius } => radius.is_finite(),
            Self::Cylinder { radius, length } => radius.is_finite() && length.is_finite(),
        }
    }
}
