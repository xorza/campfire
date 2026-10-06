use std::path::{Path, PathBuf};

use campfire_protocol::SnapshotFingerprint;

/// A session's `snapshots` directory, where each checkpoint's snapshot is named by its
/// fingerprint in hex.
#[derive(Debug, Clone)]
pub struct Snapshots(pub(crate) PathBuf);

impl Snapshots {
    pub(crate) fn dir(&self) -> &Path {
        &self.0
    }

    /// The snapshot whose fingerprint is `fingerprint`.
    pub(crate) fn file(&self, fingerprint: SnapshotFingerprint) -> PathBuf {
        self.0.join(fingerprint.to_string())
    }
}
