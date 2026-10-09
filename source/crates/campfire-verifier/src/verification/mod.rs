use std::path::Path;

use campfire_common::StateHash;
use campfire_log::ErrorReport;
use campfire_package::PackageStore;
use campfire_protocol::SessionLog;
use campfire_sim::StateRegistry;
use campfire_store::InputFile;
use tracing::warn;

use crate::replay::Replay;
use crate::verification::error::VerifyError;

pub(crate) mod error;

/// A log file verified as the verifier's binary verifies one: the packages under a directory
/// scanned, the log read and decoded, its replay started, the snapshot of each checkpoint
/// checked when a directory of them is given, and every tick replayed.
#[derive(Debug)]
pub struct Verification;

impl Verification {
    /// The state hash after the last tick of the log file at `log`, replayed with the packages
    /// under `packages`, once each checkpoint's snapshot under `snapshots`, when given, checks,
    /// each named by its fingerprint in hex. A package that does not read is logged, and costs
    /// no other.
    pub fn file(
        packages: &Path,
        log: &Path,
        snapshots: Option<&Path>,
    ) -> Result<StateHash, VerifyError> {
        let store = PackageStore::scan(packages).map_err(VerifyError::Store)?;
        for failure in store.failures() {
            warn!(dir = %failure.dir.display(), error = %ErrorReport::of(&failure.error), "a package does not read");
        }
        let bytes = InputFile::read(log, SessionLog::MAX_FILE_LEN).map_err(VerifyError::ReadLog)?;
        let decoded = SessionLog::decode(&bytes).map_err(VerifyError::Decode)?;
        let mut replay = Replay::new(decoded, &store).map_err(VerifyError::Start)?;
        if let Some(dir) = snapshots {
            for record in replay.runner().log().checkpoints() {
                let file = dir.join(record.snapshot.to_string());
                let snapshot = InputFile::read(&file, StateRegistry::MAX_SNAPSHOT_LEN)
                    .map_err(VerifyError::ReadSnapshot)?;
                replay
                    .check_snapshot(record, &snapshot)
                    .map_err(VerifyError::Snapshot)?;
            }
        }
        while replay.run_tick().map_err(VerifyError::Replay)? {}
        Ok(replay.runner().state_hash())
    }
}
