use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A share from 0 to 1, exactly as data writes it: the integer 0 or 1, or a decimal string of at
/// most nine fractional digits, kept in billionths, so `"0.7"` of 300 is 210, and `"0.5"` and
/// `"0.50"` are one share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Share(u32);

impl Share {
    /// All of an amount.
    pub const ALL: Share = Share(Share::WHOLE);

    /// The most fractional digits a share holds.
    const DIGITS: u32 = 9;

    /// All of an amount in billionths, the parts of a share of nine digits.
    const WHOLE: u32 = 10_u32.pow(Share::DIGITS);

    /// The share of `amount`, rounded down to a whole amount, exactly.
    pub fn of(self, amount: i64) -> i64 {
        let share = i128::from(amount) * i128::from(self.0) / i128::from(Share::WHOLE);
        i64::try_from(share).expect("a share of at most 1 of an amount fits")
    }

    /// The share of `value`, rounded down to the least amount a `Num` holds, exactly.
    pub fn of_num(self, value: Num) -> Num {
        let bits = i128::from(value.to_bits()) * i128::from(self.0) / i128::from(Share::WHOLE);
        Num::from_bits(i64::try_from(bits).expect("a share of at most 1 of a value fits"))
    }

    /// Whether it is no share at all.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// The share `text` writes, `[0|1][.digits]`; `None` past 1 or for anything else.
    fn parse(text: &str) -> Option<Share> {
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        let places = u32::try_from(frac.len())
            .ok()
            .filter(|&len| len <= Share::DIGITS);
        let places = places.filter(|_| matches!(int, "0" | "1") && digits(frac))?;
        if text.contains('.') && frac.is_empty() {
            return None;
        }
        let scale = 10_u32.pow(Share::DIGITS - places);
        let frac: u32 = if frac.is_empty() {
            0
        } else {
            frac.parse().ok()?
        };
        let parts = u32::from(int == "1") * Share::WHOLE + frac * scale;
        (parts <= Share::WHOLE).then_some(Share(parts))
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
            Written::Int(int @ (0 | 1)) => Some(Share(u32::from(int) * Share::WHOLE)),
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
    use campfire_common::Toml;

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
        let read = |text: &str| Toml::parse::<toml::Table>(text).unwrap()["share"].clone();
        let of = |text: &str| {
            Share::deserialize(read(text))
                .map(|share| share.of(100))
                .ok()
        };
        assert_eq!(of("share = 1"), Some(100));
        assert_eq!(of("share = 0"), Some(0));
        assert_eq!(of("share = \"0.25\""), Some(25));
        assert_eq!(of("share = 2"), None);
        // One share however data writes it.
        assert_eq!(share("0.5"), share("0.50"));
        assert_eq!(share("1.0"), Some(Share::ALL));
        assert_eq!(Share::deserialize(read("share = 1")).ok(), Some(Share::ALL));
    }
}
