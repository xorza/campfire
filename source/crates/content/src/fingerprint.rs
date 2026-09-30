use std::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};

/// A package's identity: the SHA-256 of its postcard-encoded file list, one `(path, size,
/// SHA-256)` row per file, sorted by path bytes. Any change to any file, or to the list, gives
/// another fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint([u8; 32]);

/// One row of a package's file list.
#[derive(Debug, Serialize)]
pub(crate) struct FileRow {
    /// Relative to the package root, with `/` separators.
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) sha256: [u8; 32],
}

impl Fingerprint {
    pub const fn new(bytes: [u8; 32]) -> Fingerprint {
        Fingerprint(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The fingerprint of `rows`, which are sorted by path bytes.
    pub(crate) fn of(rows: &[FileRow]) -> Fingerprint {
        debug_assert!(rows.is_sorted_by(|a, b| a.path.as_bytes() < b.path.as_bytes()));
        let list = postcard::to_allocvec(rows).expect("a file list always encodes");
        Fingerprint(Sha256::digest(list).into())
    }
}

/// Lowercase hex, as Blossom addresses a blob.
impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
