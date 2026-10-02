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
        let digits = hex.as_bytes();
        if digits.len() != 64 {
            return Err(NotHex);
        }
        let digit = |d: u8| match d {
            b'0'..=b'9' => Some(d - b'0'),
            b'a'..=b'f' => Some(d - b'a' + 10),
            _ => None,
        };
        let mut bytes = [0; 32];
        let (pairs, _) = digits.as_chunks::<2>();
        for (byte, &[high, low]) in bytes.iter_mut().zip(pairs) {
            *byte = digit(high).ok_or(NotHex)? << 4 | digit(low).ok_or(NotHex)?;
        }
        Ok(Bytes32(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::array;

    use super::*;

    #[test]
    fn it_reads_back_what_it_writes() {
        let bytes = Bytes32::new(array::from_fn(|at| u8::try_from(at * 8 + 1).unwrap()));
        let written = bytes.to_string();
        assert_eq!(&written[..8], "01091119");
        assert_eq!(written.len(), 64);
        assert_eq!(written.parse(), Ok(bytes));
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
        let encoded = postcard::to_allocvec(&bytes).unwrap();
        assert_eq!(encoded, postcard::to_allocvec(bytes.as_bytes()).unwrap());
    }
}
