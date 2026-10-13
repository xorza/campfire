use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::scalar::Scalar;

/// A speed in meters a second, positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Speed(Num);

impl Speed {
    /// `None` unless `meters_a_second` is positive.
    pub const fn new(meters_a_second: Num) -> Option<Speed> {
        if meters_a_second.to_bits() <= 0 {
            return None;
        }
        Some(Speed(meters_a_second))
    }

    /// In meters a second.
    pub const fn get(self) -> Num {
        self.0
    }
}

/// A number, or a decimal string, that is positive.
impl<'de> Deserialize<'de> for Speed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Speed, D::Error> {
        let speed = Scalar::num(deserializer)?;
        Speed::new(speed).ok_or_else(|| D::Error::custom("a speed is a positive number"))
    }
}

#[cfg(test)]
mod tests {
    use serde::de::{IntoDeserializer, value};

    use super::*;

    #[test]
    fn a_speed_is_positive() {
        assert_eq!(Speed::new(Num::EPSILON).map(Speed::get), Some(Num::EPSILON));
        assert!(Speed::new(Num::ZERO).is_none());
        assert!(Speed::new(-Num::EPSILON).is_none());
        // Read from data, a number past what a `Num` holds fails as every data number does, and
        // one that is not positive as a speed does.
        let read = |value: i64| {
            Speed::deserialize(IntoDeserializer::<value::Error>::into_deserializer(value))
                .map_err(|error| error.to_string())
        };
        assert_eq!(read(3).map(Speed::get), Ok(Num::int(3)));
        let past = read(1 << 40).unwrap_err();
        assert_eq!(past, "Int(1099511627776) is past what a number holds");
        let zero = read(0).unwrap_err();
        assert_eq!(zero, "a speed is a positive number");
    }
}
