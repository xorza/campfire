use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_common::Tick;
use campfire_package::ModePackages;
use campfire_protocol::{Checkpoint, SeedError, SessionLog};
use campfire_sim::SimWorld;

use crate::replay::error::ReplayError;
use crate::session::Session;
use crate::session::error::{ResumeError, SnapshotCheckError, StartError};
use crate::session::session_source::{ResumeAt, SessionSource, UncheckedSource};
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
    /// The match each checkpoint's snapshot restores into, built once, as the first check
    /// needs it.
    target: Option<Match>,
    packages: &'p ModePackages,
    /// The tick after the last the log holds.
    ticks: Tick,
}

impl<'p> Replay<'p> {
    /// The replay of `published` with the log's own randomness, on `packages`, the mode its terms
    /// name; an error when the log does not reveal the server seed its header commits to, or its
    /// match does not start on `packages`.
    pub fn of(published: SessionLog, packages: &'p ModePackages) -> Result<Self, StartError> {
        let seeds = published
            .revealed_seeds()
            .ok_or(StartError::Seed(SeedError::NotRevealed))?;
        let ticks = published.next_tick();
        Ok(Replay {
            game: Session::start_match(
                SessionSource::check(UncheckedSource {
                    log: published,
                    seeds,
                    packages,
                    signer: None,
                })
                .map_err(StartError::Terms)?,
            )?,
            target: None,
            packages,
            ticks,
        })
    }

    /// The replay of `published` from its checkpoint `at`, with the log's own randomness, on
    /// `packages`: the checkpoint's snapshot restored once it checks against the record, as
    /// `Session::resume` restores it, and the ticks from the checkpoint's boundary to replay. A
    /// log whose file starts at a checkpoint replays from it alone
    /// (08-sessions.md, A log file may start at a checkpoint). An error as `of` and
    /// `Session::resume` give one.
    pub fn from_checkpoint(
        published: SessionLog,
        at: ResumeAt<'_>,
        packages: &'p ModePackages,
    ) -> Result<Self, ResumeError> {
        let seeds = published
            .revealed_seeds()
            .ok_or(ResumeError::Start(StartError::Seed(SeedError::NotRevealed)))?;
        let ticks = published.next_tick();
        let source = SessionSource::check(UncheckedSource {
            log: published,
            seeds,
            packages,
            signer: None,
        })
        .map_err(|error| ResumeError::Start(StartError::Terms(error)))?;
        Ok(Replay {
            game: Session::resume_match(source, at)?,
            target: None,
            packages,
            ticks,
        })
    }

    /// Runs every tick the log holds after those that ran.
    pub fn run(&mut self) -> Result<(), ReplayError> {
        self.run_each(|_| {})
    }

    /// Runs every tick the log holds after those that ran, as `run` does, and gives `ran` the
    /// world after each.
    pub fn run_each(&mut self, ran: impl FnMut(&World)) -> Result<(), ReplayError> {
        self.run_to(self.ticks, ran)
    }

    /// Runs the ticks of the segment the replay stands in, as `run_each` runs them: to the
    /// boundary where the next starts, whose checkpoint it checks, or after the last, to the
    /// log's end and its result. So the segments of one log replay apart, each from its own
    /// checkpoint, as a verifier on many threads replays them.
    pub fn run_segment_each(&mut self, ran: impl FnMut(&World)) -> Result<(), ReplayError> {
        let start = self.game.session().next_tick();
        let log = self.game.session().log();
        let next = log
            .checkpoints()
            .map(|record| record.tick)
            .find(|&tick| tick > start);
        self.run_to(next.unwrap_or(self.ticks), ran)
    }

    /// Runs the ticks the log holds before `end`, after those that ran, giving `ran` the world
    /// after each, and checks each checkpoint on the way, the one at `end` too, and the result
    /// when `end` is the log's end: a report's replay stops at the tick its client drew
    /// (02-engine-core.md, A report replays to its tick).
    pub fn run_to(&mut self, end: Tick, mut ran: impl FnMut(&World)) -> Result<(), ReplayError> {
        let start = self.game.session().next_tick();
        assert!(
            start <= end && end <= self.ticks,
            "a replay runs to a tick between the one it stands at and the log's end"
        );
        self.game.replay_to(end, |world| {
            if world.session().next_tick() > start {
                ran(world);
            }
            Replay::check_checkpoint(world)
        })?;
        if end == self.ticks {
            self.game.check_result().map_err(ReplayError::Result)?;
        }
        Ok(())
    }

    /// Whether the state of `world` at its boundary is the one the checkpoint there, if the log
    /// holds one, records.
    fn check_checkpoint(world: &World) -> Result<(), ReplayError> {
        let next = world.session().next_tick();
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
    /// the state it records, in the one target the replay restores each into. A target a
    /// refused snapshot left partly restored is dropped.
    pub fn check_snapshot(
        &mut self,
        record: &Checkpoint,
        snapshot: &[u8],
    ) -> Result<(), SnapshotCheckError> {
        let target = match &mut self.target {
            Some(target) => {
                target.despawn_state();
                target
            }
            empty => empty.insert(self.game.session().restore_target(self.packages)),
        };
        let checked = Session::restore_checked(target, record, snapshot);
        if checked.is_err() {
            self.target = None;
        }
        checked
    }

    pub const fn game(&self) -> &Match {
        &self.game
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::replay::Replay;
    use crate::replay::error::ReplayError;
    use crate::session::session_world::SessionWorld;

    impl Replay<'_> {
        /// Runs the next tick the log holds; `false` after its last tick, once the log's result,
        /// when it holds one, holds for the state. An error when the state at the boundary where
        /// a segment starts is not the one its checkpoint records, or the result does not hold.
        pub fn run_tick(&mut self) -> Result<bool, ReplayError> {
            Replay::check_checkpoint(&self.game)?;
            if self.game.session().next_tick() == self.ticks {
                self.game.check_result().map_err(ReplayError::Result)?;
                return Ok(false);
            }
            self.game.run_tick_alone();
            Ok(true)
        }
    }
}
