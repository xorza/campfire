use crate::value32::{ContentMeaning, Meaning, Value32};

/// A package's identity: the SHA-256 of its index, `package.index`, which lists one `(path,
/// size, SHA-256)` row per file, sorted by path bytes, after its format's tag. Any change to any
/// file, or to the list, gives another fingerprint.
pub type PackageFingerprint = Value32<PackageFingerprintMeaning>;

/// The meaning of a `PackageFingerprint`.
#[derive(Debug)]
pub enum PackageFingerprintMeaning {}

impl Meaning for PackageFingerprintMeaning {
    const NAME: &'static str = "PackageFingerprint";
}

impl ContentMeaning for PackageFingerprintMeaning {}
