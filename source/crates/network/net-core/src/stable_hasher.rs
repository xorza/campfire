use core::hash::Hasher;

/// A hasher whose value is the same in every process and on every machine that feeds it the same
/// bytes: BLAKE3, its value the first 8 bytes of the digest, little-endian. A value that
/// `Hash` feeds through it enters as its type writes itself, so a peer of another endianness
/// agrees only when the type writes explicit bytes.
#[derive(Debug, Clone, Default)]
pub struct StableHasher(blake3::Hasher);

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(&self) -> u64 {
        let digest = self.0.finalize();
        let (first, _) = digest
            .as_bytes()
            .split_first_chunk::<8>()
            .expect("a digest of 32 bytes");
        u64::from_le_bytes(*first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_is_blake3s_first_eight_bytes_and_each_byte_changes_it() {
        let hash = |bytes: &[u8]| {
            let mut hasher = StableHasher::default();
            hasher.write(bytes);
            hasher.finish()
        };
        let digest = blake3::hash(b"abc");
        let expected = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
        assert_eq!(hash(b"abc"), expected);
        // Two writes are one stream, as BLAKE3 hashes its input whole.
        let mut split = StableHasher::default();
        split.write(b"a");
        split.write(b"bc");
        assert_eq!(split.finish(), expected);
        assert_ne!(hash(b"abd"), expected);
    }
}
