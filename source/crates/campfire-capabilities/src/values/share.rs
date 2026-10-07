use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A share from 0 to 1, exactly as data writes it: the integer 0 or 1, or a decimal string of at
/// most nine fractional digits, kept as a fraction of a power of ten, so `"0.7"` of 300 is 210.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share {
    parts: u64,
    whole: u64,
}

impl Share {
    /// All of an amount.
    pub const ALL: Share = Share { parts: 1, whole: 1 };

    /// The most fractional digits a share holds.
    const DIGITS: usize = 9;

    /// The share of `amount`, rounded down to a whole amount, exactly.
    pub fn of(self, amount: i64) -> i64 {
        let share = i128::from(amount) * i128::from(self.parts) / i128::from(self.whole);
        i64::try_from(share).expect("a share of at most 1 of an amount fits")
    }

    /// The share of `value`, rounded down to the least amount a `Num` holds, exactly.
    pub fn of_num(self, value: Num) -> Num {
        let bits = i128::from(value.to_bits()) * i128::from(self.parts) / i128::from(self.whole);
        Num::from_bits(i64::try_from(bits).expect("a share of at most 1 of a value fits"))
    }

    /// Whether it is no share at all.
    pub const fn is_zero(self) -> bool {
        self.parts == 0
    }

    /// The share `text` writes, `[0|1][.digits]`; `None` past 1 or for anything else.
    fn parse(text: &str) -> Option<Share> {
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if !matches!(int, "0" | "1") || frac.len() > Share::DIGITS || !digits(frac) {
            return None;
        }
        if text.contains('.') && frac.is_empty() {
            return None;
        }
        let whole = 10_u64.pow(u32::try_from(frac.len()).expect("a few digits"));
        let frac: u64 = if frac.is_empty() {
            0
        } else {
            frac.parse().ok()?
        };
        let parts = u64::from(int == "1") * whole + frac;
        (parts <= whole).then_some(Share { parts, whole })
    }
}

/// The integer 0 or 1, or a decimal string from 0 to 1, as `Share::parse` reads it.
impl<'de> Deserialize<'de> for Share {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Share, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Int(u8),
            Text(String),
        }
        let share = match Written::deserialize(deserializer)? {
            Written::Int(int @ (0 | 1)) => Some(Share {
                parts: u64::from(int),
                whole: 1,
            }),
            Written::Int(_) => None,
            Written::Text(text) => Share::parse(&text),
        };
        share.ok_or_else(|| {
            D::Error::custom("a share is 0, 1, or a decimal from 0 to 1 of at most 9 digits")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_is_exactly_the_decimal_data_writes() {
        let share = |text: &str| Share::parse(text);
        // 0.7 has no exact binary value; as a fraction of 10 it gives 70% of 300 exactly, and
        // 0.333 of 1000 is 333.
        assert_eq!(share("0.7").unwrap().of(300), 210);
        assert_eq!(share("0.333").unwrap().of(1000), 333);
        assert_eq!(share("0.999999999").unwrap().of(1_000_000_000), 999_999_999);
        assert_eq!(share("1.0").unwrap().of(7), 7);
        assert_eq!(share("0").unwrap().of(7), 0);
        // Rounded down: 0.5 of 7 is 3.
        assert_eq!(share("0.5").unwrap().of(7), 3);
        for refused in [
            "1.5",
            "2",
            "-0.5",
            "0.",
            ".5",
            "0.1234567891",
            "0.5x",
            "1.01",
        ] {
            assert_eq!(share(refused), None, "{refused}");
        }
        let read = |text: &str| toml::from_str::<toml::Table>(text).unwrap()["share"].clone();
        let of = |text: &str| {
            Share::deserialize(read(text))
                .map(|share| share.of(100))
                .ok()
        };
        assert_eq!(of("share = 1"), Some(100));
        assert_eq!(of("share = 0"), Some(0));
        assert_eq!(of("share = \"0.25\""), Some(25));
        assert_eq!(of("share = 2"), None);
    }
}
