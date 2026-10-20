use std::path::Path;

use campfire_common::ErrorReport;
use campfire_common::StateHash;
use campfire_package::ModePackages;
use campfire_package_disk::PackageStore;
use campfire_protocol::{Checkpoint, SessionLog, SessionTerms, SnapshotFingerprint};
use campfire_runner::{Replay, ResumeAt, SessionRules, SessionWorld};
use campfire_sim::{SimWorld, StateRegistry};
use campfire_store::InputFile;
use tracing::warn;

use crate::verification::error::VerifyError;

pub(crate) mod error;

/// Where a verification finds the snapshot of each checkpoint a log holds.
pub trait SnapshotSource {
    /// The snapshot of fingerprint `fingerprint`; none from a source that holds no snapshot, so
    /// the verification checks none; an error for one it does not read.
    fn snapshot(
        &mut self,
        fingerprint: SnapshotFingerprint,
    ) -> Option<Result<Vec<u8>, VerifyError>>;
}

/// No snapshot: the verification replays the log alone.
impl SnapshotSource for () {
    fn snapshot(&mut self, _: SnapshotFingerprint) -> Option<Result<Vec<u8>, VerifyError>> {
        None
    }
}

/// The snapshots of a directory, each named by its fingerprint in hex.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotFiles<'p>(pub &'p Path);

impl SnapshotSource for SnapshotFiles<'_> {
    fn snapshot(
        &mut self,
        fingerprint: SnapshotFingerprint,
    ) -> Option<Result<Vec<u8>, VerifyError>> {
        let file = self.0.join(fingerprint.to_string());
        let read = InputFile::read(&file, StateRegistry::MAX_SNAPSHOT_LEN);
        Some(read.map_err(VerifyError::ReadSnapshot))
    }
}

/// The state hash after the last tick of the log file at `log`, replayed with the packages
/// under `packages`, once each checkpoint's snapshot under `snapshots`, when given, checks,
/// each named by its fingerprint in hex: the verifier's binary's verification. A package that
/// does not read is logged, and costs no other.
pub fn verify_file(
    packages: &Path,
    log: &Path,
    snapshots: Option<&Path>,
) -> Result<StateHash, VerifyError> {
    let store = PackageStore::scan(packages).map_err(VerifyError::Store)?;
    for failure in store.failures() {
        warn!(dir = %failure.dir.display(), error = %ErrorReport::of(&failure.error), "a package does not read");
    }
    let bytes = InputFile::read(log, SessionLog::MAX_FILE_LEN).map_err(VerifyError::ReadLog)?;
    match snapshots {
        Some(dir) => verify_log(&bytes, &store, &mut SnapshotFiles(dir)),
        None => verify_log(&bytes, &store, &mut ()),
    }
}

/// The state hash after the last tick of the log file `log`, replayed with the packages of
/// `store` its terms name, once the snapshot `snapshots` gives of each checkpoint, if any,
/// checks: the log decoded, its replay started, from tick 0 or from the snapshot of the
/// checkpoint the file starts at, which it needs, each later snapshot checked, and every tick
/// replayed. A file that starts at a checkpoint trusts only what the server signed there: the
/// snapshot's fingerprint and its state hash are the record's
/// (08-sessions.md, A log file may start at a checkpoint).
pub fn verify_log(
    log: &[u8],
    store: &PackageStore,
    snapshots: &mut impl SnapshotSource,
) -> Result<StateHash, VerifyError> {
    let decoded = SessionLog::decode(log).map_err(VerifyError::Decode)?;
    let packages = mode_packages(store, &decoded.header().terms)?;
    let first = decoded
        .starts_at()
        .map(|record| (record.segment, record.snapshot));
    let mut replay = match first {
        None => Replay::of(decoded, &packages).map_err(VerifyError::Start)?,
        Some((segment, fingerprint)) => {
            let snapshot = snapshots
                .snapshot(fingerprint)
                .ok_or(VerifyError::MissingSnapshot { segment })??;
            let at = ResumeAt {
                segment,
                snapshot: &snapshot,
            };
            Replay::from_checkpoint(decoded, at, &packages).map_err(VerifyError::Resume)?
        }
    };
    let records: Vec<Checkpoint> = replay
        .game()
        .session()
        .log()
        .checkpoints()
        .filter(|record| first.is_none_or(|(segment, _)| record.segment != segment))
        .cloned()
        .collect();
    for record in &records {
        let Some(snapshot) = snapshots.snapshot(record.snapshot) else {
            break;
        };
        replay
            .check_snapshot(record, &snapshot?)
            .map_err(VerifyError::Snapshot)?;
    }
    replay.run().map_err(VerifyError::Replay)?;
    Ok(replay.game().state_hash())
}

/// The mode and the dependencies `terms` name, from `store`; an error for terms of another
/// release first, as its packages need not read in this one.
fn mode_packages(store: &PackageStore, terms: &SessionTerms) -> Result<ModePackages, VerifyError> {
    SessionRules::check_release(terms).map_err(VerifyError::Release)?;
    store
        .mode_packages(terms.mode, &terms.map, &terms.dependencies)
        .map_err(VerifyError::Packages)
}
