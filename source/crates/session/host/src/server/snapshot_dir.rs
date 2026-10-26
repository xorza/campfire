use std::path::PathBuf;

use campfire_protocol::SnapshotFingerprint;
use campfire_sim::StateRegistry;
use campfire_store::{DurableDir, DurableFile, InputFile, PathError, ReadError};

use crate::faults::error::SnapshotWriteError;

/// A session's `snapshots` directory, where each checkpoint's snapshot is named by its
/// fingerprint in hex.
#[derive(Debug, Clone)]
pub struct SnapshotDir(pub(crate) PathBuf);

impl SnapshotDir {
    /// The snapshots of the directory `path`, as a verifier takes a session's.
    pub const fn at(path: PathBuf) -> Self {
        Self(path)
    }

    /// Writes `snapshot`, whose fingerprint is `fingerprint`, durably, the directory made when
    /// missing, named by the fingerprint.
    pub(crate) fn write(
        &self,
        snapshot: &[u8],
        fingerprint: SnapshotFingerprint,
    ) -> Result<(), SnapshotWriteError> {
        debug_assert_eq!(fingerprint, SnapshotFingerprint::of(snapshot));
        DurableDir::create(&self.0)?;
        DurableFile::write(&self.file(fingerprint), snapshot)?;
        Ok(())
    }

    /// Reads the snapshot whose fingerprint is `fingerprint`, as long as a snapshot may be.
    pub fn read(&self, fingerprint: SnapshotFingerprint) -> Result<Vec<u8>, PathError<ReadError>> {
        InputFile::read(&self.file(fingerprint), StateRegistry::MAX_SNAPSHOT_LEN)
    }

    /// The snapshot whose fingerprint is `fingerprint`.
    fn file(&self, fingerprint: SnapshotFingerprint) -> PathBuf {
        self.0.join(fingerprint.to_string())
    }
}
