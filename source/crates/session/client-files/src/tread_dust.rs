use serde::{Deserialize, Serialize};

/// How a tread's particle systems follow its unit's drive (wrecks.md, A tread throws dust while it
/// drives): they run while the unit moves along the ground faster than `moving` metres a second,
/// and stop as it stands; their particles' velocity across and up, and their bursts' counts, each
/// scaled by its speed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreadDust {
    pub moving: f32,
    pub across: SpeedScale,
    pub rise: SpeedScale,
    pub count: SpeedScale,
}

/// A scale by a speed in metres a second: `base` plus `per_speed` times the speed, at most `most`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedScale {
    pub base: f32,
    pub per_speed: f32,
    pub most: f32,
}

impl TreadDust {
    /// No tread throws dust.
    pub const NONE: Self = Self {
        moving: 0.0,
        across: SpeedScale::NONE,
        rise: SpeedScale::NONE,
        count: SpeedScale::NONE,
    };

    /// Whether it holds: a speed and scales of no number below 0, each finite.
    pub fn holds(&self) -> bool {
        let speed = |speed: f32| speed >= 0.0 && speed.is_finite();
        speed(self.moving)
            && [self.across, self.rise, self.count]
                .iter()
                .all(SpeedScale::holds)
    }

    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

impl SpeedScale {
    const NONE: Self = Self {
        base: 0.0,
        per_speed: 0.0,
        most: 0.0,
    };

    /// The scale at `speed` metres a second.
    pub const fn at(&self, speed: f32) -> f32 {
        self.per_speed.mul_add(speed, self.base).min(self.most)
    }

    /// Whether it holds: no number below 0, each finite.
    fn holds(&self) -> bool {
        [self.base, self.per_speed, self.most]
            .iter()
            .all(|value| *value >= 0.0 && value.is_finite())
    }
}
