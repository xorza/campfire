use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// An amount from 0 to a positive maximum, such as health or a resource pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Meter {
    current: Num,
    max: Num,
    /// What regen added beyond whole bits, in parts of a bit as many as the ticks a second; 0
    /// while full or empty.
    carry: u32,
}

impl Meter {
    /// A full meter of `max`; `None` unless `max` is positive.
    pub(crate) const fn new(max: Num) -> Option<Meter> {
        if max.to_bits() <= 0 {
            return None;
        }
        Some(Meter {
            current: max,
            max,
            carry: 0,
        })
    }

    pub(crate) const fn current(self) -> Num {
        self.current
    }

    pub(crate) const fn max(self) -> Num {
        self.max
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.current.to_bits() == 0
    }

    pub(crate) const fn fill(&mut self) {
        self.current = self.max;
    }

    /// Sets the maximum to `max`, which is positive: a rise raises the current amount as much, a
    /// fall keeps it but at most the new maximum.
    pub(crate) fn set_max(&mut self, max: Num) {
        debug_assert!(max > Num::ZERO, "a meter's maximum is positive");
        if max > self.max {
            self.current += max - self.max;
        } else {
            self.current = self.current.min(max);
        }
        self.max = max;
        if self.current == max {
            self.carry = 0;
        }
    }

    /// Adds a tick's share of `per_second` at `hz` ticks a second, within 0 and the maximum:
    /// the bits a second divided by `hz`, the remainder carried to the next tick, so `hz` ticks
    /// add exactly `per_second`. At either end the remainder is dropped.
    pub(crate) fn regen(&mut self, per_second: Num, hz: u32) {
        let total = i128::from(per_second.to_bits()) + i128::from(self.carry);
        let hz = i128::from(hz);
        let gain = total.div_euclid(hz);
        self.carry = u32::try_from(total.rem_euclid(hz)).expect("a remainder below the rate");
        let current =
            (i128::from(self.current.to_bits()) + gain).clamp(0, i128::from(self.max.to_bits()));
        self.current = Num::from_bits(i64::try_from(current).expect("within the maximum"));
        if self.current == self.max || current == 0 {
            self.carry = 0;
        }
    }

    /// Adds `amount`, which is not negative, up to the maximum.
    pub(crate) fn add(&mut self, amount: Num) {
        debug_assert!(
            amount >= Num::ZERO,
            "a meter adds an amount that is not negative"
        );
        self.current = self
            .current
            .checked_add(amount)
            .map_or(self.max, |sum| sum.min(self.max));
        if self.current == self.max {
            self.carry = 0;
        }
    }

    /// Takes `amount`, which is not negative, down to 0 at the least.
    pub(crate) fn take(&mut self, amount: Num) {
        debug_assert!(
            amount >= Num::ZERO,
            "a meter takes an amount that is not negative"
        );
        self.current = (self.current - amount).max(Num::ZERO);
    }
}

/// A snapshot is untrusted, so a meter outside 0 to a positive maximum fails to decode.
impl<'de> Deserialize<'de> for Meter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Meter, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            current: Num,
            max: Num,
            carry: u32,
        }
        let Fields {
            current,
            max,
            carry,
        } = Fields::deserialize(deserializer)?;
        if max <= Num::ZERO || current < Num::ZERO || current > max {
            return Err(D::Error::custom("meter outside 0 to a positive maximum"));
        }
        Ok(Meter {
            current,
            max,
            carry,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_meter_holds_0_to_its_positive_max() {
        assert_eq!(Meter::new(Num::ZERO), None);
        assert_eq!(Meter::new(-Num::EPSILON), None);
        let mut meter = Meter::new(Num::int(10)).unwrap();
        assert_eq!((meter.current(), meter.max()), (Num::int(10), Num::int(10)));
        meter.take(Num::int(4));
        assert_eq!(meter.current(), Num::int(6));
        meter.take(Num::int(7));
        assert!(meter.is_empty());
        // Adding 3 to 0 of 10 gives 3; 9 more stops at 10; the most a number holds, at 10 too.
        for (added, current) in [
            (Num::int(3), Num::int(3)),
            (Num::int(9), Num::int(10)),
            (Num::MAX, Num::int(10)),
        ] {
            meter.add(added);
            assert_eq!(meter.current(), current);
        }
        meter.take(Num::int(10));
        // A rise of 5 to 15 raises 0 to 5; a fall to 3 cuts it to 3; a rise to 20 raises it by
        // 17, to 20; a fall to 12 cuts it to 12.
        for (max, current) in [(15, 5), (3, 3), (20, 20), (12, 12)] {
            meter.set_max(Num::int(max));
            assert_eq!(
                (meter.current(), meter.max()),
                (Num::int(current), Num::int(max))
            );
        }

        // 1 a second at 3 ticks a second, from 2 of 12: 2²⁴ bits ÷ 3 is 5 592 405, a third of a
        // bit left; the third tick carries two thirds, 1 more: 2²⁴ in all, exactly 1.
        meter.take(Num::int(10));
        for gain in [5_592_405, 5_592_405, 5_592_406] {
            let before = meter.current();
            meter.regen(Num::ONE, 3);
            assert_eq!(meter.current() - before, Num::from_bits(gain));
        }
        assert_eq!(meter.current(), Num::int(3));
        // Full, or empty, it drops the remainder.
        meter.set_max(Num::int(3));
        meter.regen(Num::ONE, 3);
        assert_eq!((meter.current(), meter.carry), (Num::int(3), 0));
        meter.regen(-Num::int(9), 3);
        assert_eq!((meter.current(), meter.carry), (Num::ZERO, 0));

        for (current, max, reads) in [
            (0, 10, true),
            (10, 10, true),
            (11, 10, false),
            (-1, 10, false),
            (0, 0, false),
        ] {
            let encoded =
                postcard::to_allocvec(&(Num::int(current), Num::int(max), 0_u32)).unwrap();
            let decoded = postcard::from_bytes::<Meter>(&encoded);
            assert_eq!(decoded.is_ok(), reads, "{current} of {max}");
        }
    }
}
