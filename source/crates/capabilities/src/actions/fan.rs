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
mod tests {
    use super::*;

    #[test]
    fn a_fan_turns_each_projectile_by_its_share_of_the_spread_rounded_once() {
        let fan = |count, spread| Fan {
            count: NonZeroU8::new(count).unwrap(),
            spread_deg: Num::from_int(spread).unwrap(),
        };
        // π is 52707179 / 2²⁴. Three over 30° turn −15°, 0 and 15°: π / 12 is 4392264.92, to
        // 4392265. Rounded twice, as π / 180 = 292817.66 to 292818 times 15, it was 4392270.
        assert_eq!(Num::PI.to_bits(), 52_707_179);
        let three = [0, 1, 2].map(|at| fan(3, 30).turn(at).to_bits());
        assert_eq!(three, [-4_392_265, 0, 4_392_265]);
        // Two over 90° turn ∓45°: π / 4 is 13176794.75, to 13176795. One turns none.
        let two = [0, 1].map(|at| fan(2, 90).turn(at).to_bits());
        assert_eq!(two, [-13_176_795, 13_176_795]);
        assert_eq!(fan(1, 90).turn(0), Num::ZERO);
    }
}
