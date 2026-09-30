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
