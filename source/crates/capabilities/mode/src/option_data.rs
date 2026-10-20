use campfire_script::{DataSection, DataTable};
use campfire_values::Scalar;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::OptionDataError;

/// An option of the mode's `[options]` (03-game-scripting.md, A mode declares its options): a
/// param whose value a session's host picks among `values`, in their order, and the place among
/// them of the one a session of no pick takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionData {
    values: Vec<Scalar>,
    default: u8,
}

impl OptionData {
    /// The most values an option holds, as a session's terms give a pick's place in a byte.
    pub const MAX_VALUES: usize = 1 << u8::BITS;

    /// The option of `values`, in their order, which a session of no pick takes `default` of; an
    /// error for no value, more than `MAX_VALUES`, one value twice, or a default that is none of
    /// them.
    pub fn new(values: Vec<Scalar>, default: Scalar) -> Result<Self, OptionDataError> {
        if values.is_empty() || values.len() > Self::MAX_VALUES {
            return Err(OptionDataError::Count);
        }
        for (at, &value) in values.iter().enumerate() {
            if values[..at].iter().any(|&held| alike(held, value)) {
                return Err(OptionDataError::Repeated);
            }
        }
        let option = Self { values, default: 0 };
        let default = option.place(default).ok_or(OptionDataError::NoDefault)?;
        Ok(Self { default, ..option })
    }

    /// Its values, in their order.
    pub fn values(&self) -> &[Scalar] {
        &self.values
    }

    /// The place of the value a session of no pick takes.
    pub const fn default_place(&self) -> u8 {
        self.default
    }

    /// The value at `at`; none past its values.
    pub fn value(&self, at: u8) -> Option<Scalar> {
        self.values.get(usize::from(at)).copied()
    }

    /// The place of `value` among its values, a decimal and an integer of one number alike; none
    /// for a value it lacks.
    pub fn place(&self, value: Scalar) -> Option<u8> {
        let at = self.values.iter().position(|&held| alike(held, value))?;
        Some(u8::try_from(at).expect("the read bounds an option's values"))
    }
}

/// An option as data writes it, which reads and writes the same fields: its default by its value.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    values: Vec<Scalar>,
    default: Scalar,
}

impl Serialize for OptionData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Fields {
            values: self.values.clone(),
            default: self.values[usize::from(self.default)],
        }
        .serialize(serializer)
    }
}

/// Data is untrusted, so an option of no value, of more than `MAX_VALUES`, of one value twice, or
/// whose default is none of its values, fails to read.
impl<'de> Deserialize<'de> for OptionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Fields { values, default } = Fields::deserialize(deserializer)?;
        Self::new(values, default).map_err(D::Error::custom)
    }
}

impl DataSection for OptionData {
    const TABLE: DataTable = DataTable::new("An option, `[options.<name>]`");
}

/// Whether `a` and `b` are one number: equal, or a decimal and an integer of one value.
fn alike(a: Scalar, b: Scalar) -> bool {
    a == b || matches!((a.to_num(), b.to_num()), (Some(a), Some(b)) if a == b)
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use campfire_math::Num;

    use super::*;

    #[test]
    fn an_option_reads_its_values_and_its_default_and_refuses_what_no_session_may_pick() {
        let read = |text: &str| Toml::parse::<OptionData>(text);
        let money = read("values = [5000, 10000, 20000, 50000]\ndefault = 10000").unwrap();
        let values = [5000, 10000, 20000, 50000].map(Scalar::Int);
        assert_eq!(money.values(), values);
        assert_eq!(money.default_place(), 1);
        // A decimal of a value's number is that value; a value it lacks, and a place past its
        // values, are none.
        assert_eq!(money.place(Scalar::Decimal(Num::int(50000))), Some(3));
        assert_eq!(money.place(Scalar::Int(7)), None);
        assert_eq!(money.value(3), Some(Scalar::Int(50000)));
        assert_eq!(money.value(4), None);
        let half = read("values = [\"0.5\", 2]\ndefault = \"0.5\"").unwrap();
        assert_eq!(half.default_place(), 0);
        // Each writes its default as its value, and reads back to itself; `new` makes the same.
        for option in [&money, &half] {
            assert_eq!(read(&Toml::write(option)).as_ref(), Ok(option));
        }
        assert_eq!(
            OptionData::new(values.to_vec(), Scalar::Int(10000)),
            Ok(money)
        );
        // 256 values read, 257 do not.
        let many = |count: i64| {
            let values: Vec<String> = (0..count).map(|value| value.to_string()).collect();
            read(&format!("values = [{}]\ndefault = 0", values.join(", ")))
        };
        assert_eq!(many(256).unwrap().values().len(), 256);
        for (text, refusal) in [
            (
                "values = []\ndefault = 0",
                "an option holds from 1 to 256 values",
            ),
            (
                "values = [1, 2, 1]\ndefault = 1",
                "an option holds each value once",
            ),
            (
                "values = [1, \"1\"]\ndefault = 1",
                "an option holds each value once",
            ),
            (
                "values = [1, 2]\ndefault = 3",
                "an option's default is one of its values",
            ),
        ] {
            let message = read(text).unwrap_err().message().to_owned();
            assert_eq!(message, refusal, "{text}");
        }
        assert_eq!(
            many(257).unwrap_err().message(),
            "an option holds from 1 to 256 values"
        );
    }
}
