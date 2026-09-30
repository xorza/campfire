use std::fmt::Write;

/// Lowercase hex, as Nostr writes keys and ids.
pub(crate) fn encode(bytes: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("a String takes any write");
    }
    hex
}

/// 32 bytes from exactly 64 lowercase hex digits, the one spelling `encode` writes.
pub(crate) fn decode(hex: &str) -> Option<[u8; 32]> {
    let digits = hex.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let digit = |d: u8| match d {
        b'0'..=b'9' => Some(d - b'0'),
        b'a'..=b'f' => Some(d - b'a' + 10),
        _ => None,
    };
    let mut bytes = [0; 32];
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, &[high, low]) in bytes.iter_mut().zip(pairs) {
        *byte = digit(high)? << 4 | digit(low)?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use std::array;

    use super::*;

    #[test]
    fn decode_takes_back_what_encode_writes() {
        let bytes: [u8; 32] = array::from_fn(|at| u8::try_from(at * 8 + 1).unwrap());
        let written = encode(&bytes);
        assert_eq!(&written[..8], "01091119");
        assert_eq!(decode(&written), Some(bytes));
        for flawed in [
            "",
            "0",
            &written[1..],
            &format!("{written}0"),
            &written.replace('9', "g"),
        ] {
            assert_eq!(decode(flawed), None, "{flawed}");
        }
    }
}
