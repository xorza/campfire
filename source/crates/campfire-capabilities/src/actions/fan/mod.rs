use std::num::NonZeroU8;

use campfire_math::Num;

/// How a delivery launches its projectiles: `count` of them, spread evenly over `spread_deg`
/// degrees around the aim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fan {
    pub(crate) count: NonZeroU8,
    pub(crate) spread_deg: Num,
}

impl Fan {
    /// The turn from the aim of projectile `at` of the fan, in radians: `spread · (2 · at −
    /// (count − 1)) / (2 · (count − 1))` degrees, so the fan spreads evenly about the aim, rounded
    /// once; none for a fan of one.
    pub(crate) fn turn(self, at: u8) -> Num {
        let count = i64::from(self.count.get());
        if count == 1 {
            return Num::ZERO;
        }
        debug_assert!(i64::from(at) < count);
        self.spread_deg
            .checked_mul_int(2 * i64::from(at) - (count - 1))
            .and_then(|turn| turn.checked_mul_div_int(Num::PI, 360 * (count - 1)))
            .expect("the load bounds a fan's spread to a full turn")
    }
}

#[cfg(test)]
mod tests;
