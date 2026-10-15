use campfire_common::{ContentMeaning, Meaning, Value32};

/// A snapshot's identity: the SHA-256 of its bytes, as a Blossom server addresses a blob, so a
/// snapshot is shared as is. A checkpoint record names its snapshot by it, and the server names
/// the snapshot's file by it in hex.
pub type SnapshotFingerprint = Value32<SnapshotFingerprintMeaning>;

/// The meaning of a `SnapshotFingerprint`.
#[derive(Debug)]
pub enum SnapshotFingerprintMeaning {}

impl Meaning for SnapshotFingerprintMeaning {
    const NAME: &'static str = "SnapshotFingerprint";
}

impl ContentMeaning for SnapshotFingerprintMeaning {}
