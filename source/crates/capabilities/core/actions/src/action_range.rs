use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// How far an action reaches. In data: meters, or `global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionRange {
    Meters(Num),
    Global,
}
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
        match Written::deserialize(deserializer) {
            Ok(Written::Meters(meters)) if meters >= Num::ZERO => Ok(Self::Meters(meters)),
            Ok(Written::Word(Word::Global)) => Ok(Self::Global),
            _ => Err(Error::custom("a range is meters from 0, or global")),
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
        assert_eq!(read("5").unwrap(), ActionRange::Meters(Num::int(5)));
        assert_eq!(
            read("\"1.25\"").unwrap(),
            ActionRange::Meters(Num::int(5) / Num::int(4))
        );
        assert_eq!(read("0").unwrap(), ActionRange::Meters(Num::ZERO));
        assert_eq!(read("\"global\"").unwrap(), ActionRange::Global);
        for refused in ["-1", "\"-0.5\"", "\"far\"", "1.5", "1099511627776"] {
            assert!(read(refused).is_err(), "{refused}");
        }
    }
}
