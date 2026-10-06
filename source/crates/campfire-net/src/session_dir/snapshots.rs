use std::path::PathBuf;

use campfire_protocol::SnapshotFingerprint;
use campfire_store::{DurableError, DurableFile};

/// A session's `snapshots` directory, where each checkpoint's snapshot is named by its
/// fingerprint in hex.
#[derive(Debug, Clone)]
pub struct Snapshots(pub(crate) PathBuf);

impl Snapshots {
    /// Writes `snapshot` durably, the directory made when missing, named by its fingerprint;
    /// the fingerprint.
    pub(crate) fn write(&self, snapshot: &[u8]) -> Result<SnapshotFingerprint, DurableError> {
        let fingerprint = SnapshotFingerprint::of(snapshot);
        DurableFile::create_dir(&self.0)?;
        DurableFile::write(&self.file(fingerprint), snapshot)?;
        Ok(fingerprint)
    }

    /// The snapshot whose fingerprint is `fingerprint`.
    pub(crate) fn file(&self, fingerprint: SnapshotFingerprint) -> PathBuf {
        self.0.join(fingerprint.to_string())
    }
}
