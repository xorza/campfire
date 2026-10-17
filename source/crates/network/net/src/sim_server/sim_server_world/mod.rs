use bevy_app::AppExit;
use bevy_ecs::world::World;
use campfire_log::{ErrorReport, LogEvent};
use campfire_protocol::SessionTerms;
use campfire_runner::{Session, SessionWorld};
use campfire_sim::SimWorld;
use tracing::error;

use crate::events::session_written::SessionWritten;
use crate::faults::Faults;
use crate::faults::error::SnapshotWriteError;
use crate::faults::fault::Fault;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::door::Door;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::lobby::Lobby;
use crate::sim_server::pending_restore::PendingRestore;
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_phase::ServerPhase;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::SessionDir;

/// What the sim server module gives a world, for every crate.
pub trait SimServerWorld {
    /// Opens the server's session: `opening`'s lobby, open to its players from the next frame
    /// on, or the session it restores in the next frame.
    fn open_session(&mut self, opening: SessionOpening);

    /// Ends the session before the next tick, as the mode ended the match or aborted when it did
    /// not: logs the record of the checkpoint on its thread first; then the result, which the
    /// server key signs; reveals the seed; and from then on runs no tick and logs nothing, as each
    /// such system checks that the session runs. The server then publishes the log. An error when
    /// the checkpoint's snapshot was not written, as the log can then not be published: the session
    /// ends with no result.
    fn end_session(&mut self) -> Result<(), SnapshotWriteError>;

    /// Whether the session went back to a save, and its server is to start again on
    /// its data, which restores the match from it.
    fn reload_wanted(&self) -> bool;

    /// How the server of the session exits in this frame, when it does: as `Faults`
    /// says, once a worker failed, which for its journal or a checkpoint's snapshot is with
    /// `ExitStatus::Storage`; before the match started, at once when `stop` asks it to. Once the
    /// match started, when `stop` asks it to or every player left, it ends the session, as the
    /// mode ended the match or aborted, and closes its journal, whose last sync's failure ends it
    /// as one in a frame does; it then publishes the log in its `ServerDir`, and exits, with an
    /// error when the log is not written, as the session it holds is lost.
    fn exit_due(&mut self, stop: StopRequest) -> Option<AppExit>;
}

/// The session a server opens: a new one, its lobby open to its players, or the one a stop
/// ended, to restore. A session past the restore window, or one that ended, has its log
/// published instead, and a new one opens.
#[derive(Debug)]
pub enum SessionOpening {
    New(Box<Lobby>),
    Restored(Box<PendingRestore>),
}

impl SessionOpening {
    /// The session's terms.
    pub fn terms(&self) -> &SessionTerms {
        match self {
            Self::New(lobby) => lobby.terms(),
            Self::Restored(restore) => restore.session.terms(),
        }
    }

    /// The setup of the server that opens it.
    pub fn server(&self) -> &ServerSetup {
        match self {
            Self::New(lobby) => lobby.server(),
            Self::Restored(restore) => &restore.server,
        }
    }
}

/// Whether the server's owner asked it to stop, as a local server's client does as it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRequest {
    Asked,
    NotAsked,
}

/// What the sim server module gives a world, for its own crate.
pub(crate) trait SimServerWorldCrate {
    /// Loads the save that starts segment `segment` in the session: logs the record of
    /// the checkpoint on its thread first, then goes back in the log, as `SessionLog::load`
    /// says, which the journal logs. The match then runs no tick and logs nothing: its server
    /// starts again on its data, which restores the match from the save, once `reload_wanted`
    /// says so. An error when the checkpoint's snapshot was not written.
    fn load_session_save(&mut self, segment: u32) -> Result<(), SnapshotWriteError>;
}

impl SimServerWorld for World {
    fn open_session(&mut self, opening: SessionOpening) {
        let phase = match opening {
            SessionOpening::New(lobby) => {
                self.insert_resource(*lobby);
                ServerPhase::Lobby
            }
            SessionOpening::Restored(restore) => {
                self.insert_resource(*restore);
                ServerPhase::Restoring
            }
        };
        ServerPhase::go(self, phase);
    }

    fn end_session(&mut self) -> Result<(), SnapshotWriteError> {
        if self.contains_resource::<Checkpoints>() {
            Checkpoints::settle(self)?;
            self.remove_resource::<Checkpoints>();
        }
        let result = self.session_result();
        let mut session = self.resource_mut::<Session>();
        session
            .record_result(result)
            .expect("the server's own result holds");
        session.reveal_seed();
        Ok(())
    }

    fn reload_wanted(&self) -> bool {
        ServerPhase::now(self) == ServerPhase::Reloading
    }

    fn exit_due(&mut self, stop: StopRequest) -> Option<AppExit> {
        JournalWatch::report_failure(self);
        if let Some(exit) = Faults::apply(self) {
            return Some(exit);
        }
        if !matches!(
            ServerPhase::now(self),
            ServerPhase::Running | ServerPhase::Reloading
        ) {
            return (stop == StopRequest::Asked).then_some(AppExit::Success);
        }
        if self.resource::<Session>().log().result().is_none() {
            // Every slot a player controlled left, and no player holds a link.
            if stop == StopRequest::NotAsked && !Door::empty(self) {
                return None;
            }
            if let Err(error) = self.end_session() {
                self.resource_mut::<Faults>().report(Fault::Snapshot(error));
                return Faults::apply(self);
            }
        }
        self.session_mut().close_journal();
        JournalWatch::report_failure(self);
        if let Some(exit) = Faults::apply(self) {
            return Some(exit);
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

impl SimServerWorldCrate for World {
    fn load_session_save(&mut self, segment: u32) -> Result<(), SnapshotWriteError> {
        Checkpoints::settle(self)?;
        self.resource_mut::<Session>()
            .load(segment)
            .expect("a save of the session's log");
        ServerPhase::go(self, ServerPhase::Reloading);
        Ok(())
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_bots::{BotScript, OrderScript};
    use campfire_common::{PlayerSlot, Tick};
    use campfire_runner::SessionWorld;
    use campfire_units::UnitTypeBook;

    use crate::faults::Faults;
    use crate::faults::fault::Fault;
    use crate::sim_server::bot_driver::BotDriver;
    use crate::sim_server::checkpoints::Checkpoints;
    use crate::sim_server::player_link::PlayerLink;
    use crate::sim_server::receipts::Receipts;
    use crate::sim_server::server_bots::SlotBot;

    /// What a server's world gives the tests and benches of any crate.
    pub trait SimServerWorldInternals {
        /// Makes the server give its receipts in its next update, as if their second passed.
        fn give_receipts(&mut self);

        /// Holds the server's next round of receipts until `give_receipts`, so a test, not the
        /// real clock, says when they go out.
        fn hold_receipts(&mut self);

        /// The slots of the plan's bots of `slots` that play their script from tick `next` of
        /// the world's session on, as a server that starts or restores builds its driver.
        fn planned_bots(&self, slots: Vec<SlotBot>, next: Tick) -> Vec<PlayerSlot>;

        /// Waits for the checkpoint on its thread in the session, whose server keeps its
        /// files, and logs its record; reports the fault when its snapshot was not written.
        fn settle_checkpoint(&mut self);

        /// Makes a checkpoint due at the boundary before `tick` in the session, whose
        /// server keeps its files.
        fn request_checkpoint(&mut self, tick: Tick);

        /// The slot of the player the client link `link` carries the inputs of; none for a link
        /// of no player.
        fn link_slot(&self, link: Entity) -> Option<PlayerSlot>;

        /// Whether the log refused one of the messages of the client link `link`; none for a
        /// link of no player.
        fn link_refused(&self, link: Entity) -> Option<bool>;
    }

    impl SimServerWorldInternals for World {
        fn give_receipts(&mut self) {
            self.resource_mut::<Receipts>().due_now();
        }

        fn hold_receipts(&mut self) {
            self.resource_mut::<Receipts>().hold();
        }

        fn planned_bots(&self, slots: Vec<SlotBot>, next: Tick) -> Vec<PlayerSlot> {
            let types = self.resource::<UnitTypeBook>().types();
            let script = |script: &OrderScript| BotScript::new(script, types).unwrap();
            let planned = BotDriver::planned(slots, self.session().log(), next, script);
            planned.iter().map(|driven| driven.slot).collect()
        }

        fn settle_checkpoint(&mut self) {
            if let Err(error) = Checkpoints::settle(self) {
                self.resource_mut::<Faults>().report(Fault::Snapshot(error));
            }
        }

        fn request_checkpoint(&mut self, tick: Tick) {
            self.resource_mut::<Checkpoints>().request(tick);
        }

        fn link_slot(&self, link: Entity) -> Option<PlayerSlot> {
            self.get::<PlayerLink>(link).map(|link| link.slot())
        }

        fn link_refused(&self, link: Entity) -> Option<bool> {
            self.get::<PlayerLink>(link).map(|link| link.refused())
        }
    }
}

#[cfg(test)]
mod tests;
