use sha2::{Digest, Sha256};

use crate::value32::{ContentMeaning, Meaning, Value32};

/// The SHA-256 of a file's bytes, as a package's index lists each file's, and as a Blossom server
/// names a blob (design 05, Hashes, ids and keys).
pub type ContentHash = Value32<ContentHashMeaning>;

/// The meaning of a `ContentHash`.
#[derive(Debug)]
pub enum ContentHashMeaning {}

impl Meaning for ContentHashMeaning {
    const NAME: &'static str = "ContentHash";
}

impl ContentMeaning for ContentHashMeaning {}

/// The SHA-256 of bytes given in parts, as a file streams past.
#[derive(Debug, Default, Clone)]
pub struct ContentHasher(Sha256);

impl ContentHasher {
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The hash of every part given.
    pub fn finish(self) -> ContentHash {
        ContentHash::new(self.0.finalize().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_content_hash_is_the_sha256_of_its_bytes_whole_or_in_parts() {
        // SHA-256's own test vector, of "abc".
        let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(ContentHash::of(b"abc"), ContentHash::of_hex(hex));
        let mut hasher = ContentHasher::default();
        for part in [&b"a"[..], b"", b"bc"] {
            hasher.update(part);
        }
        assert_eq!(hasher.finish(), ContentHash::of(b"abc"));
        assert_ne!(ContentHash::of(b"abd"), ContentHash::of(b"abc"));
    }
}
