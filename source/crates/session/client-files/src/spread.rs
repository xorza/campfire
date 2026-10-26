use serde::{Deserialize, Serialize};

/// A value drawn anew, uniformly, from `low` to `high` each time an effect takes it, as a
/// particle system draws each of its values for each system and each particle; one of `low` equal
/// to `high` is that value. A file writes it `[low, high]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f32; 2]", into = "[f32; 2]")]
pub struct Spread {
    pub low: f32,
    pub high: f32,
}

impl Spread {
    pub const ZERO: Self = Self::of(0.0);

    /// The spread of the one value `value`.
    pub const fn of(value: f32) -> Self {
        Self {
            low: value,
            high: value,
        }
    }

    /// Whether both its ends are finite.
    pub const fn is_finite(self) -> bool {
        self.low.is_finite() && self.high.is_finite()
    }
}

impl From<[f32; 2]> for Spread {
    fn from([low, high]: [f32; 2]) -> Self {
        Self { low, high }
    }
}

impl From<Spread> for [f32; 2] {
    fn from(spread: Spread) -> Self {
        [spread.low, spread.high]
    }
}
