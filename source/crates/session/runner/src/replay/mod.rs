use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_common::Tick;
use campfire_package::ModePackages;
use campfire_protocol::{Checkpoint, SeedError, SessionLog};
use campfire_sim::{MatchRate, SimWorld};

use crate::match_build::MatchBuild;
use crate::replay::error::ReplayError;
use crate::session::Session;
use crate::session::error::{SnapshotCheckError, StartError};
use crate::session::session_source::{SessionSource, UncheckedSource};
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

    /// Runs every tick the log holds after those that ran.
    pub fn run(&mut self) -> Result<(), ReplayError> {
        self.run_each(|_| {})
    }

    /// Runs every tick the log holds after those that ran, as `run` does, and gives `ran` the
    /// world after each.
    pub fn run_each(&mut self, mut ran: impl FnMut(&World)) -> Result<(), ReplayError> {
        let start = self.game.session().next_tick();
        self.game.replay_to(self.ticks, |world| {
            if world.session().next_tick() > start {
                ran(world);
            }
            Replay::check_checkpoint(world)
        })?;
        self.game.check_result().map_err(ReplayError::Result)
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
            empty => {
                let session = self.game.session();
                let header = session.log().header();
                let seats = u32::try_from(header.seats.len()).expect("the log counts seats in u32");
                let rate = MatchRate::new(header.terms.tick_hz);
                let picks = &session.checked().picks;
                empty.insert(MatchBuild::restore_target(
                    self.packages,
                    rate,
                    seats,
                    picks,
                ))
            }
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
