use campfire_common::Tick;
use campfire_package::{ModePackages, PackageStore};
use campfire_protocol::{Checkpoint, SeedError, SessionLog, SnapshotFingerprint};
use campfire_runner::{Runner, Session, StartError};

use crate::replay::error::ReplayError;
use crate::replay::error::SnapshotCheckError;

pub(crate) mod error;

/// A published session log replayed in a bare `World`, one tick at a time, with the packages its
/// terms name, each segment with its own seed. Decoding the log checked every chain link and
/// signature; the replay seals its ticks again, which gives each tick exactly the inputs the
/// server applied, with no check done twice, and checks the state at each checkpoint, and the
/// result after the last tick.
#[derive(Debug)]
pub struct Replay {
    runner: Runner,
    packages: ModePackages,
    /// The tick after the last the log holds.
    ticks: Tick,
}

impl Replay {
    /// The replay with the log's own randomness and the mode its terms name, from `store`; an
    /// error when the log does not reveal the server seed its header commits to, names another
    /// engine release, or a mode or dependency `store` does not hold, or the mode does not run
    /// at the log's tick rate.
    pub fn new(published: SessionLog, store: &PackageStore) -> Result<Replay, StartError> {
        let seeds = published
            .revealed_seeds()
            .ok_or(StartError::Seed(SeedError::NotRevealed))?;
        let terms = &published.header().terms;
        let packages = Session::packages(store, terms)?;
        let ticks = published.next_tick();
        Ok(Replay {
            runner: Runner::new(published.rewound(), seeds, &packages)?,
            packages,
            ticks,
        })
    }

    /// Runs the next tick the log holds; `false` after its last tick, once the log's result, when
    /// it holds one, holds for the state. An error when the state at the boundary where a segment
    /// starts is not the one its checkpoint records, or the result does not hold.
    pub fn run_tick(&mut self) -> Result<bool, ReplayError> {
        let next = self.runner.log().next_tick();
        if let Some(record) = self.runner.log().checkpoint_at(next) {
            let replayed = self.runner.state_hash();
            if replayed != record.state_hash {
                return Err(ReplayError::Checkpoint {
                    segment: record.segment,
                    tick: next,
                    logged: record.state_hash,
                    replayed,
                });
            }
        }
        if next == self.ticks {
            self.runner.check_result().map_err(ReplayError::Result)?;
            return Ok(false);
        }
        self.runner.run_tick();
        Ok(true)
    }

    /// Whether `snapshot` is the one `record`, a checkpoint of the log, names, and restores to
    /// the state it records.
    pub fn check_snapshot(
        &self,
        record: &Checkpoint,
        snapshot: &[u8],
    ) -> Result<(), SnapshotCheckError> {
        if SnapshotFingerprint::of(snapshot) != record.snapshot {
            return Err(SnapshotCheckError::Fingerprint);
        }
        let header = self.runner.log().header();
        let restored = Session::snapshot_hash(&self.packages, header, snapshot)
            .map_err(SnapshotCheckError::Restore)?;
        if restored != record.state_hash {
            return Err(SnapshotCheckError::Hash {
                logged: record.state_hash,
                restored,
            });
        }
        Ok(())
    }

    pub const fn runner(&self) -> &Runner {
        &self.runner
    }
}
