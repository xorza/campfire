use crate::value32::{ContentMeaning, Meaning, Value32};

/// A package's identity: the SHA-256 of its index, `package.index`, which lists one `(path,
/// size, SHA-256)` row per file, sorted by path bytes, after its format's tag. Any change to any
/// file, or to the list, gives another fingerprint.
pub type Fingerprint = Value32<FingerprintMeaning>;

/// The meaning of a `Fingerprint`.
#[derive(Debug)]
pub enum FingerprintMeaning {}

impl Meaning for FingerprintMeaning {
    const NAME: &'static str = "Fingerprint";
}

impl ContentMeaning for FingerprintMeaning {}
