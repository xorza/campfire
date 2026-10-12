use bevy_app::AppExit;
use bevy_ecs::world::{Mut, World};
use bevy_time::{Real, Time};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::{ErrorReport, LogEvent};
use campfire_package::ModePackages;
use campfire_protocol::{Outcome, ServerInput};
use campfire_runner::{Session, SessionWorld};
use campfire_sim::{SimWorld, StateRegistry};
use campfire_store::{DurableError, InputFile, PathError};
use lightyear::prelude::LocalTimeline;
use tracing::error;

use crate::events::session_written::SessionWritten;
use crate::faults::Faults;
use crate::faults::fault::Fault;
use crate::faults::faults_world::FaultsWorldCrate;
use crate::match_clock::MatchClock;
use crate::sim_server::bot_driver::BotDriver;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::door::Door;
use crate::sim_server::error::RestoreMatchError;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::seats::Seats;
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::server_signer::ServerSigner;
use crate::sim_server::session_dir::{RestoredSession, SessionDir, SessionFiles};
use crate::sim_server::tick_hashes::TickHashes;
use crate::sim_server::{self, FrameStart, Reload, SimServer};

/// What the sim server module gives a world, for every crate.
pub trait SimServerWorld {
    /// Restores the match of `restored`, a session a crash or a failed journal ended, of the mode
    /// `packages` holds, on the server of `server`: builds it from the snapshot of the latest
    /// checkpoint whose record the log holds, or from tick 0 with none, and replays its log from
    /// there to the tick the server stopped before, keeping each tick's state hash while the
    /// world holds `TickHashes`; a checkpoint begun with no record is taken again as the replay
    /// reaches its boundary, and one due at the last boundary, as a save the mode asked for,
    /// begins there. It logs `Disconnected` for each slot a player controls, whose grace
    /// period runs from then, and runs the match on from the next fixed tick, so the ticks the
    /// stop lost take no time in the sim. The journal goes on from its last record, and
    /// `JournalWatch` watches it. No client is linked: the players come back through the door.
    fn restore_match(
        &mut self,
        restored: RestoredSession,
        packages: &ModePackages,
        server: &ServerSetup,
        bots: ServerBots,
    ) -> Result<(), RestoreMatchError>;

    /// Whether every slot a player controlled has left, and no player holds a link: a session
    /// that started then ends.
    fn players_gone(&self) -> bool;

    /// Ends the session before the next tick as `outcome`: logs the record of the
    /// checkpoint on its thread first; then the result, which the server key signs; reveals the
    /// seed; and from then on runs no tick and logs nothing, as each such system checks that the
    /// session runs. The server then publishes the log. An error when the checkpoint's snapshot
    /// was not written, as the log can then not be published: the session ends with no result.
    fn end_session(&mut self, outcome: Outcome) -> Result<(), PathError<DurableError>>;

    /// Loads the save that starts segment `segment` in the session: logs the record of
    /// the checkpoint on its thread first, then goes back in the log, as `SessionLog::load`
    /// says, which the journal logs. The match then runs no tick and logs nothing: its server
    /// starts again on its data, which restores the match from the save, once `reload_wanted`
    /// says so. An error when the checkpoint's snapshot was not written.
    fn load_session_save(&mut self, segment: u32) -> Result<(), PathError<DurableError>>;

    /// Whether the session went back to a save, and its server is to start again on
    /// its data, which restores the match from it.
    fn reload_wanted(&self) -> bool;

    /// Waits for the checkpoint on its thread in the session, whose server keeps its
    /// files, and logs its record; reports the fault when its snapshot was not written.
    fn settle_checkpoint(&mut self);

    /// Makes a checkpoint due at the boundary before `tick` in the session, whose
    /// server keeps its files.
    fn request_checkpoint(&mut self, tick: Tick);

    /// How the server of the session exits in this frame, when it does: as `Faults`
    /// says, once a worker failed, which for its journal or a checkpoint's snapshot is with
    /// `ExitStatus::Storage`; before the match started, at once when `stop` asks it to. Once the
    /// match started, when `stop` asks it to or every player left, it ends the session, as the
    /// mode ended the match or aborted; it then publishes the log in its `ServerDir`, and exits,
    /// with an error when the log is not written, as the session it holds is lost.
    fn exit_due(&mut self, stop: bool) -> Option<AppExit>;
}

impl SimServerWorld for World {
    fn restore_match(
        &mut self,
        restored: RestoredSession,
        packages: &ModePackages,
        server: &ServerSetup,
        bots: ServerBots,
    ) -> Result<(), RestoreMatchError> {
        let RestoredSession {
            private,
            log,
            files: SessionFiles { journal, snapshots },
            ..
        } = restored;
        let terms = log.header().terms.clone();
        SimServer::bound_frames(self, &terms);
        let ticks = log.next_tick();
        let slots = log.slot_count();
        let seeds = private.seed_chain.seeds();
        let latest = log
            .checkpoints()
            .last()
            .map(|record| (record.segment, record.snapshot));
        if let Some((segment, fingerprint)) = latest {
            let snapshot = InputFile::read(
                &snapshots.file(fingerprint),
                StateRegistry::MAX_SNAPSHOT_LEN,
            )
            .map_err(RestoreMatchError::ReadSnapshot)?;
            self.resume_session(log.rewound(), seeds, packages, segment, &snapshot)
                .map_err(RestoreMatchError::Resume)?;
        } else {
            self.start_session(log.rewound(), seeds, packages)
                .map_err(RestoreMatchError::Start)?;
        }
        self.insert_resource(JournalWatch(journal.watch()));
        self.session_mut().resume_journal(Box::new(journal));
        let signer = ServerSigner::new(server.key, server.entropy);
        loop {
            Checkpoints::take_again(self, &snapshots, &signer)
                .map_err(RestoreMatchError::WriteSnapshot)?;
            if self.session().log().next_tick() >= ticks {
                break;
            }
            self.run_tick_alone();
            if self.contains_resource::<TickHashes>() {
                sim_server::record_hash(self);
            }
        }
        SimServer::open_door(self, terms, server, None);
        Checkpoints::start(self, snapshots, signer);
        // The stop may have come after the tick before this boundary, before the save it made due
        // here began.
        Checkpoints::begin(self);
        let players: Vec<PlayerSlot> = self.resource::<Session>().log().player_slots().collect();
        self.resource_scope(|world, mut session: Mut<'_, Session>| {
            let signer = world.resource::<ServerSigner>();
            for &slot in &players {
                signer
                    .serve(&mut session, ServerInput::Disconnected { slot })
                    .expect("the server's own input holds");
            }
        });
        let now = self.resource::<Time<Real>>().elapsed();
        self.insert_resource(Seats::gone(slots, players.into_iter(), now));
        self.insert_resource(Receipts::new(slots));
        let next = self.resource::<Session>().log().next_tick();
        let start = self.resource::<LocalTimeline>().tick() + 1;
        self.insert_resource(MatchClock::resumed(start, next));
        self.insert_resource(FrameStart(next));
        let driver = BotDriver::new(bots, next, self);
        self.insert_resource(driver);
        Ok(())
    }

    fn players_gone(&self) -> bool {
        Door::empty(self)
    }

    fn end_session(&mut self, outcome: Outcome) -> Result<(), PathError<DurableError>> {
        if self.contains_resource::<Checkpoints>() {
            Checkpoints::settle(self)?;
            self.remove_resource::<Checkpoints>();
        }
        let result = self.session_result_as(outcome);
        let id = self.session().log().session_id();
        let signature = self.resource::<ServerSigner>().sign_result(&result, id);
        let mut session = self.session_mut();
        session
            .record_result(result, &signature)
            .expect("the server's own result holds");
        session.reveal_seed();
        Ok(())
    }

    fn load_session_save(&mut self, segment: u32) -> Result<(), PathError<DurableError>> {
        Checkpoints::settle(self)?;
        self.resource_mut::<Session>()
            .load(segment)
            .expect("a save of the session's log");
        self.insert_resource(Reload);
        Ok(())
    }

    fn reload_wanted(&self) -> bool {
        self.contains_resource::<Reload>()
    }

    fn settle_checkpoint(&mut self) {
        if let Err(error) = Checkpoints::settle(self) {
            self.resource_mut::<Faults>().report(Fault::Snapshot(error));
        }
    }

    fn request_checkpoint(&mut self, tick: Tick) {
        self.resource_mut::<Checkpoints>().request(tick);
    }

    fn exit_due(&mut self, stop: bool) -> Option<AppExit> {
        if let Some(exit) = self.apply_faults() {
            return Some(exit);
        }
        if !self.contains_resource::<MatchClock>() {
            return stop.then_some(AppExit::Success);
        }
        if self.resource::<Session>().log().result().is_none() {
            if !stop && !self.players_gone() {
                return None;
            }
            let outcome = self.outcome();
            if let Err(error) = self.end_session(outcome) {
                self.resource_mut::<Faults>().report(Fault::Snapshot(error));
                return self.apply_faults();
            }
        }
        let session = self.resource::<Session>();
        let id = session.log().session_id();
        let hash = self.state_hash();
        let data = self.resource::<ServerDir>();
        Some(match SessionDir::publish(data, session.log()) {
            Ok(file) => {
                SessionWritten {
                    session: id,
                    file,
                    hash,
                }
                .log();
                AppExit::Success
            }
            Err(error) => {
                error!(session = %id, error = %ErrorReport::of(&error), "could not write the session log");
                AppExit::error()
            }
        })
    }
}

#[cfg(test)]
mod tests;
