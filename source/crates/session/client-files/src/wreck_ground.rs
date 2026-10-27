use serde::{Deserialize, Serialize};

/// How a wreck meets the ground as it falls (wrecks.md, Wrecks move by the client's step): the
/// gravity it falls by, in metres a second squared, none above 0; the share of its speed down
/// the ground takes as it lands, its stiffness; the share of its speed along the ground and of
/// its spin each step on the ground takes, its friction; and the speed, in metres a second, below
/// which a wreck on the ground rests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WreckGround {
    pub gravity: f32,
    pub stiffness: f32,
    pub friction: f32,
    pub rest: f32,
}

impl WreckGround {
    /// No wreck falls.
    pub const NONE: Self = Self {
        gravity: 0.0,
        stiffness: 0.0,
        friction: 0.0,
        rest: 0.0,
    };

    /// Whether it holds: a gravity of no pull up, shares of 0 to 1, a rest of no speed below 0,
    /// each finite.
    pub fn holds(&self) -> bool {
        let share = |share: f32| (0.0..=1.0).contains(&share);
        self.gravity <= 0.0
            && self.gravity.is_finite()
            && share(self.stiffness)
            && share(self.friction)
            && self.rest >= 0.0
            && self.rest.is_finite()
    }

    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}
