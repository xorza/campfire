//! A 32-byte value that writes, and reads back, as lowercase hex, as Nostr writes keys and ids.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::bytes32::error::NotHex;

pub(crate) mod error;

/// 32 bytes, such as a key, a hash or an id. It writes as exactly 64 lowercase hex digits, the
/// one spelling it reads back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bytes32([u8; 32]);

impl Bytes32 {
    pub const fn new(bytes: [u8; 32]) -> Bytes32 {
        Bytes32(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn get(self) -> [u8; 32] {
        self.0
    }

    /// The value the 64 lowercase hex digits `hex` write, as a literal in the code gives it; a
    /// text from outside reads by `FromStr`, which refuses a flawed one.
    pub const fn of_hex(hex: &str) -> Bytes32 {
        match Bytes32::parse(hex.as_bytes()) {
            Some(bytes) => bytes,
            None => panic!("32 bytes are 64 lowercase hex digits"),
        }
    }

    /// The value the 64 lowercase hex digits `digits` write; none for any other text.
    const fn parse(digits: &[u8]) -> Option<Bytes32> {
        if digits.len() != 64 {
            return None;
        }
        let mut bytes = [0; 32];
        let mut at = 0;
        while at < 32 {
            let (Some(high), Some(low)) = (
                Bytes32::digit(digits[2 * at]),
                Bytes32::digit(digits[2 * at + 1]),
            ) else {
                return None;
            };
            bytes[at] = (high << 4) | low;
            at += 1;
        }
        Some(Bytes32(bytes))
    }

    /// The value of the lowercase hex digit `byte`; none for any other byte.
    const fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }
}

impl fmt::Display for Bytes32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Bytes32 {
    type Err = NotHex;

    fn from_str(hex: &str) -> Result<Bytes32, NotHex> {
        Bytes32::parse(hex.as_bytes()).ok_or(NotHex)
    }
}

#[cfg(test)]
mod tests {
    use std::array;

    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn it_reads_back_what_it_writes() {
        let bytes = Bytes32::new(array::from_fn(|at| u8::try_from(at * 8 + 1).unwrap()));
        let written = bytes.to_string();
        assert_eq!(&written[..8], "01091119");
        assert_eq!(written.len(), 64);
        assert_eq!(written.parse(), Ok(bytes));
        assert_eq!(Bytes32::of_hex(&written), bytes);
        for flawed in [
            "",
            "0",
            &written[1..],
            &format!("{written}0"),
            &written.replace('9', "g"),
            &written.to_uppercase(),
        ] {
            assert_eq!(flawed.parse::<Bytes32>(), Err(NotHex), "{flawed}");
        }
        let encoded = Binary::encode(&bytes);
        assert_eq!(encoded, Binary::encode(bytes.as_bytes()));
    }
}
