use std::fmt;

/// A package's identity: the SHA-256 of its postcard-encoded file list, one `(path, size,
/// SHA-256)` row per file, sorted by path bytes. Any change to any file, or to the list, gives
/// another fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub const fn new(bytes: [u8; 32]) -> Fingerprint {
        Fingerprint(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// In lowercase hex.
impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_shows_as_lowercase_hex() {
        let mut bytes = [0; 32];
        bytes[0] = 0xAB;
        bytes[31] = 0x05;
        let shown = Fingerprint::new(bytes).to_string();
        assert_eq!(shown, format!("ab{}05", "00".repeat(30)));
    }
}
