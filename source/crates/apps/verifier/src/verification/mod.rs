use std::path::Path;

use campfire_common::ErrorReport;
use campfire_common::StateHash;
use campfire_host::SnapshotDir;
use campfire_package::ModePackages;
use campfire_package_disk::PackageStore;
use campfire_protocol::{Checkpoint, SessionLog, SessionTerms};
use campfire_runner::{Replay, ResumeAt, SessionRules, SessionWorld};
use campfire_sim::SimWorld;
use campfire_store::InputFile;
use tracing::warn;

use crate::verification::error::VerifyError;

pub(crate) mod error;

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
    let mut snapshots = snapshots.map(|dir| SnapshotDir::at(dir.to_owned()));
    verify_log(&bytes, &store, snapshots.as_mut())
}

/// The state hash after the last tick of the log file `log`, replayed with the packages of
/// `store` its terms name, once each checkpoint's snapshot in the directory `snapshots`, when
/// given, checks: the log decoded, its replay started, from tick 0 or from the snapshot of the
/// checkpoint the file starts at, which it needs, each later snapshot checked, and every tick
/// replayed. A file that starts at a checkpoint trusts only what the server signed there: the
/// snapshot's fingerprint and its state hash are the record's
/// (08-sessions.md, A log file may start at a checkpoint).
pub fn verify_log(
    log: &[u8],
    store: &PackageStore,
    mut snapshots: Option<&mut SnapshotDir>,
) -> Result<StateHash, VerifyError> {
    let decoded = SessionLog::decode(log).map_err(VerifyError::Decode)?;
    let packages = mode_packages(store, &decoded.header().terms)?;
    let first = decoded
        .starts_at()
        .map(|record| (record.segment, record.snapshot));
    let mut replay = match first {
        None => Replay::of(decoded, &packages).map_err(VerifyError::Start)?,
        Some((segment, fingerprint)) => {
            let dir = snapshots
                .as_deref_mut()
                .ok_or(VerifyError::MissingSnapshot { segment })?;
            let snapshot = dir.read(fingerprint).map_err(VerifyError::ReadSnapshot)?;
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
    if let Some(dir) = snapshots {
        for record in &records {
            let snapshot = dir
                .read(record.snapshot)
                .map_err(VerifyError::ReadSnapshot)?;
            replay
                .check_snapshot(record, &snapshot)
                .map_err(VerifyError::Snapshot)?;
        }
    }
    replay.run().map_err(VerifyError::Replay)?;
    Ok(replay.game().state_hash())
}

/// The mode and the dependencies `terms` name, from `store`; an error for terms of another
/// release first, as its packages need not read in this one.
fn mode_packages(store: &PackageStore, terms: &SessionTerms) -> Result<ModePackages, VerifyError> {
    SessionRules::check_release(terms).map_err(VerifyError::Release)?;
    ModePackages::found(store, terms.mode, &terms.map, &terms.dependencies)
        .map_err(VerifyError::Packages)
}
