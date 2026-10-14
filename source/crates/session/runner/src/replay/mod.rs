use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_common::Tick;
use campfire_package::ModePackages;
use campfire_protocol::{Checkpoint, SeedError, SessionLog, SnapshotFingerprint};
use campfire_sim::SimWorld;

use crate::replay::error::{ReplayError, SnapshotCheckError};
use crate::session::Session;
use crate::session::error::StartError;
use crate::session::session_world::SessionWorld;

pub(crate) mod error;

/// A published session log replayed in a world of its own, one tick at a time, with the packages
/// its terms name, each segment with its own seed. Decoding the log checked every chain link and
/// signature; the replay seals its ticks again, which gives each tick exactly the inputs the
/// server applied, with no check done twice, and checks the state at each checkpoint, and the
/// result after the last tick.
#[derive(Debug)]
pub struct Replay<'p> {
    game: Match,
    packages: &'p ModePackages,
    /// The tick after the last the log holds.
    ticks: Tick,
}

impl<'p> Replay<'p> {
    /// The replay of `published` with the log's own randomness, on `packages`, the mode its terms
    /// name; an error when the log does not reveal the server seed its header commits to, or its
    /// match does not start on `packages`.
    pub fn of(published: SessionLog, packages: &'p ModePackages) -> Result<Replay<'p>, StartError> {
        let seeds = published
            .revealed_seeds()
            .ok_or(StartError::Seed(SeedError::NotRevealed))?;
        let ticks = published.next_tick();
        Ok(Replay {
            game: Session::start_match(published.rewound(), seeds, packages)?,
            packages,
            ticks,
        })
    }

    /// Runs the next tick the log holds; `false` after its last tick, once the log's result, when
    /// it holds one, holds for the state. An error when the state at the boundary where a segment
    /// starts is not the one its checkpoint records, or the result does not hold.
    pub fn run_tick(&mut self) -> Result<bool, ReplayError> {
        Replay::check_checkpoint(&self.game)?;
        if self.game.session().log().next_tick() == self.ticks {
            self.game.check_result().map_err(ReplayError::Result)?;
            return Ok(false);
        }
        self.game.run_tick_alone();
        Ok(true)
    }

    /// Runs every tick the log holds after those that ran, as `run_tick` runs each.
    pub fn run(&mut self) -> Result<(), ReplayError> {
        self.run_each(|_| {})
    }

    /// Runs every tick the log holds after those that ran, as `run` does, and gives `ran` the
    /// world after each.
    fn run_each(&mut self, mut ran: impl FnMut(&World)) -> Result<(), ReplayError> {
        let start = self.game.session().log().next_tick();
        self.game.replay_to(self.ticks, |world| {
            if world.session().log().next_tick() > start {
                ran(world);
            }
            Replay::check_checkpoint(world)
        })?;
        self.game.check_result().map_err(ReplayError::Result)
    }

    /// Whether the state of `world` at its boundary is the one the checkpoint there, if the log
    /// holds one, records.
    fn check_checkpoint(world: &World) -> Result<(), ReplayError> {
        let next = world.session().log().next_tick();
        let Some(record) = world.session().log().checkpoint_at(next) else {
            return Ok(());
        };
        let replayed = world.state_hash();
        if replayed != record.state_hash {
            return Err(ReplayError::Checkpoint {
                segment: record.segment,
                tick: next,
                logged: record.state_hash,
                replayed,
            });
        }
        Ok(())
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
        let header = self.game.session().log().header();
        let restored = Session::snapshot_hash(self.packages, header, snapshot)
            .map_err(SnapshotCheckError::Restore)?;
        if restored != record.state_hash {
            return Err(SnapshotCheckError::Hash {
                logged: record.state_hash,
                restored,
            });
        }
        Ok(())
    }

    pub const fn game(&self) -> &Match {
        &self.game
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::harness::hash_trail::HashTrail;
    use crate::replay::Replay;
    use crate::replay::error::ReplayError;

    impl Replay<'_> {
        /// Runs every tick the log holds after those that ran, as `run` does; the state after
        /// each, which the match that wrote the log must have held.
        pub fn trail(&mut self) -> Result<HashTrail, ReplayError> {
            let mut trail = HashTrail::default();
            self.run_each(|world| trail.record(world))?;
            Ok(trail)
        }
    }
}
