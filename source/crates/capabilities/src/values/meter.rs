use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// An amount from 0 to a positive maximum, such as health or a resource pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Meter {
    current: Num,
    max: Num,
}

impl Meter {
    /// A full meter of `max`; `None` unless `max` is positive.
    pub(crate) const fn new(max: Num) -> Option<Meter> {
        if max.to_bits() <= 0 {
            return None;
        }
        Some(Meter { current: max, max })
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
        #[derive(Deserialize)]
        struct Fields {
            current: Num,
            max: Num,
        }
        let Fields { current, max } = Fields::deserialize(deserializer)?;
        if max <= Num::ZERO || current < Num::ZERO || current > max {
            return Err(D::Error::custom("meter outside 0 to a positive maximum"));
        }
        Ok(Meter { current, max })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    #[test]
    fn a_meter_holds_0_to_its_positive_max() {
        assert_eq!(Meter::new(Num::ZERO), None);
        assert_eq!(Meter::new(-Num::EPSILON), None);
        let mut meter = Meter::new(num(10)).unwrap();
        assert_eq!((meter.current(), meter.max()), (num(10), num(10)));
        meter.take(num(4));
        assert_eq!(meter.current(), num(6));
        meter.take(num(7));
        assert!(meter.is_empty());
        for (current, max, reads) in [
            (0, 10, true),
            (10, 10, true),
            (11, 10, false),
            (-1, 10, false),
            (0, 0, false),
        ] {
            let encoded = postcard::to_allocvec(&(num(current), num(max))).unwrap();
            let decoded = postcard::from_bytes::<Meter>(&encoded);
            assert_eq!(decoded.is_ok(), reads, "{current} of {max}");
        }
    }
}
