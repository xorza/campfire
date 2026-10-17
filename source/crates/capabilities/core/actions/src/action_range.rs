use campfire_math::Num;
use campfire_values::Distance;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// How far an action reaches. In data: meters, or `global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionRange {
    Meters(Distance),
    Global,
}

impl ActionRange {
    /// A reach of `meters`, which a caller gives never negative.
    pub const fn meters(meters: Num) -> Self {
        match Distance::new(meters) {
            Some(meters) => Self::Meters(meters),
            None => panic!("a range is never negative"),
        }
    }
}

/// Every value that is not a range fails with the one message, a word or a number past what a
/// number holds as a negative number does.
impl<'de> Deserialize<'de> for ActionRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Word {
            Global,
        }
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Meters(Num),
            Word(Word),
        }
        let refused = || D::Error::custom("a range is meters from 0, or global");
        match Written::deserialize(deserializer).map_err(|_| refused())? {
            Written::Meters(meters) => Distance::new(meters).map(Self::Meters).ok_or_else(refused),
            Written::Word(Word::Global) => Ok(Self::Global),
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_range_is_meters_from_0_as_an_integer_or_a_decimal_or_global() {
        #[derive(Debug, Deserialize)]
        struct Field {
            range: ActionRange,
        }
        let read = |value: &str| {
            Toml::parse::<Field>(&format!("range = {value}")).map(|field| field.range)
        };
        assert_eq!(read("5").unwrap(), ActionRange::meters(Num::int(5)));
        assert_eq!(
            read("\"1.25\"").unwrap(),
            ActionRange::meters(Num::int(5) / Num::int(4))
        );
        assert_eq!(read("0").unwrap(), ActionRange::Meters(Distance::ZERO));
        assert_eq!(read("\"global\"").unwrap(), ActionRange::Global);
        for refused in ["-1", "\"-0.5\"", "\"far\"", "1.5", "1099511627776"] {
            let error = read(refused).unwrap_err().to_string();
            assert!(
                error.contains("a range is meters from 0, or global"),
                "{refused}: {error}"
            );
        }
    }
}
