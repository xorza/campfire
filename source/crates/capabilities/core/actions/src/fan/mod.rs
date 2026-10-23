use std::num::{NonZeroI64, NonZeroU8};

use campfire_math::{Num, Rounding};

/// How a delivery launches its projectiles: `count` of them, spread evenly over `spread_deg`
/// degrees around the aim, the first and the last at its ends; over a full turn, a ring, each
/// `360 / count` degrees on from the one before, the first at the aim, as the ends of a full turn
/// are one direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fan {
    pub count: NonZeroU8,
    pub(crate) spread_deg: Num,
}

impl Fan {
    /// One alone, at the aim.
    pub const ONE: Self = Self {
        count: NonZeroU8::MIN,
        spread_deg: Num::ZERO,
    };

    /// A fan of `count` over `spread_deg` degrees, from 0 to 360, which its delivery's read
    /// checks.
    pub fn new(count: NonZeroU8, spread_deg: Num) -> Self {
        debug_assert!(
            (Num::ZERO..=Num::int(360)).contains(&spread_deg),
            "a spread from 0 to 360 degrees"
        );
        Self { count, spread_deg }
    }

    /// The turn from the aim of projectile `at` of the fan, in radians, rounded once: `spread · (2
    /// · at − (count − 1)) / (2 · (count − 1))` degrees, so the fan spreads evenly about the aim;
    /// `360 · at / count` degrees over a full turn; none for a fan of one.
    pub fn turn(self, at: u8) -> Num {
        let count = NonZeroI64::from(self.count).get();
        if count == 1 {
            return Num::ZERO;
        }
        debug_assert!(i64::from(at) < count);
        if self.spread_deg == Num::int(360) {
            return Num::PI
                .checked_mul_div(
                    Num::int(2 * i64::from(at)),
                    Num::int(count),
                    Rounding::NearestEven,
                )
                .expect("a ring's turn is less than a full one");
        }
        self.spread_deg
            .checked_mul_int(2 * i64::from(at) - (count - 1))
            .and_then(|turn| {
                turn.checked_mul_div(Num::PI, Num::int(360 * (count - 1)), Rounding::NearestEven)
            })
            .expect("a delivery's data bounds a fan's spread to a full turn")
    }
}

#[cfg(test)]
mod tests;
