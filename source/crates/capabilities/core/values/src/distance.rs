use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A distance in meters, from 0: an action's range, a placement rule's distance, a gather's
/// bounce.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Distance(Num);

impl Distance {
    pub const ZERO: Self = Self(Num::ZERO);

    /// `None` for a negative `meters`.
    pub const fn new(meters: Num) -> Option<Self> {
        if meters.to_bits() < 0 {
            return None;
        }
        Some(Self(meters))
    }

    /// In meters.
    pub const fn get(self) -> Num {
        self.0
    }
}

/// As its number, which `Deserialize` reads.
impl Serialize for Distance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A number, or a decimal string, from 0.
impl<'de> Deserialize<'de> for Distance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let meters = Num::deserialize(deserializer)?;
        Self::new(meters).ok_or_else(|| D::Error::custom("a distance is a number from 0"))
    }
}

#[cfg(test)]
mod tests {
    use serde::de::{IntoDeserializer, value};

    use super::*;

    #[test]
    fn a_distance_is_from_0() {
        assert_eq!(Distance::new(Num::ZERO), Some(Distance::ZERO));
        assert_eq!(
            Distance::new(Num::EPSILON).map(Distance::get),
            Some(Num::EPSILON)
        );
        assert!(Distance::new(-Num::EPSILON).is_none());
        // Read from data, a number past what a `Num` holds fails as every data number does, and
        // a negative one as a distance does.
        let read = |value: i64| {
            Distance::deserialize(IntoDeserializer::<value::Error>::into_deserializer(value))
                .map_err(|error| error.to_string())
        };
        assert_eq!(read(3).map(Distance::get), Ok(Num::int(3)));
        let past = read(1 << 40).unwrap_err();
        assert_eq!(past, "1099511627776 is past what a number holds");
        assert_eq!(read(-1).unwrap_err(), "a distance is a number from 0");
    }
}
