use std::sync::Arc;

use bevy_app::AppExit;
use bevy_ecs::world::World;
use campfire_common::ErrorReport;
use campfire_log::LogEvent;
use campfire_mode::ModeWorld;
use campfire_package::ModePackages;
use campfire_protocol::SessionTerms;
use campfire_runner::{Session, SessionWorld};
use campfire_sim::SimWorld;
use tracing::{Span, error, info_span};

use crate::sim_server::door::Door;
use crate::sim_server::lobby::Lobby;
use crate::sim_server::pending_restore::PendingRestore;
use campfire_host::Checkpoints;
use campfire_host::Fault;
use campfire_host::JournalWatch;
use campfire_host::ServerDir;
use campfire_host::ServerPhase;
use campfire_host::ServerSetup;
use campfire_host::SessionDir;
use campfire_host::SnapshotWriteError;
use campfire_host::WorkerFaults;
use campfire_host::{SessionEnd, SessionWritten};

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

    /// How the server of the session exits in this frame, when it does: as `WorkerFaults`
    /// says, once a worker failed, which for its journal or a checkpoint's snapshot is with
    /// `ExitStatus::Storage`; before the match started, at once when `stop` asks it to. Once the
    /// match started, when `stop` asks it to or, of a plan that places a player, every player
    /// left, it ends the session, as the mode ended the match or aborted, and closes its journal,
    /// whose last sync's failure ends it as one in a frame does; it then publishes the log in its
    /// `ServerDir`, and exits, with an error when the log is not written, as the session it holds
    /// is lost.
    fn exit_due(&mut self, stop: StopRequest) -> Option<AppExit>;

    /// What ends the session in this frame of a match that started, as `exit_due` ends it: the
    /// match, once its mode ended it, when `stop` asks or every player left; a stop of the owner,
    /// as `stop` asks; every player's leave, of a plan that places a player; none while it runs
    /// on.
    fn session_end(&self, stop: StopRequest) -> Option<SessionEnd>;
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

    /// The root span of the session's server, which names the session in each line its thread
    /// logs inside it, as its app runs there.
    pub fn span(&self) -> Span {
        info_span!("session", id = %self.terms().session_id())
    }

    /// The packages of the session's mode.
    pub fn packages(&self) -> &Arc<ModePackages> {
        match self {
            Self::New(lobby) => lobby.packages(),
            Self::Restored(restore) => &restore.packages,
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
        phase.go(self);
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

    fn session_end(&self, stop: StopRequest) -> Option<SessionEnd> {
        let log = self.resource::<Session>().log();
        if log.result().is_none() {
            // Every player slot a player controlled left, and no player holds a link; a session of
            // bots alone runs on until its match ends or a stop (08-sessions.md, A local session
            // may hold bots alone).
            let bots_alone = !log.header().terms.places_a_player();
            if stop == StopRequest::NotAsked && (bots_alone || !Door::is_empty(self)) {
                return None;
            }
        }
        Some(if log.result().is_some() || self.match_end().is_some() {
            SessionEnd::MatchEnded
        } else if stop == StopRequest::Asked {
            SessionEnd::Stopped
        } else {
            SessionEnd::PlayersLeft
        })
    }

    fn exit_due(&mut self, stop: StopRequest) -> Option<AppExit> {
        JournalWatch::report_failure(self);
        if let Some(exit) = WorkerFaults::apply(self) {
            return Some(exit);
        }
        if ServerPhase::now(self) != ServerPhase::Running {
            return (stop == StopRequest::Asked).then_some(AppExit::Success);
        }
        let end = self.session_end(stop)?;
        if self.resource::<Session>().log().result().is_none()
            && let Err(error) = self.end_session()
        {
            self.resource_mut::<WorkerFaults>()
                .report(Fault::Snapshot(error));
            return WorkerFaults::apply(self);
        }
        self.session_mut().close_journal();
        JournalWatch::report_failure(self);
        if let Some(exit) = WorkerFaults::apply(self) {
            return Some(exit);
        }
        let log = self.resource::<Session>().log();
        let id = log.session_id();
        let revealed = log
            .revealed_seeds()
            .expect("a session that ends reveals its seed");
        let last = revealed
            .seed(log.segment())
            .expect("the seeds reach the last segment");
        let hash = self.state_hash();
        let data = self.resource::<ServerDir>();
        Some(match SessionDir::publish(data, id, last) {
            Ok(file) => {
                SessionWritten {
                    session: id,
                    end,
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_common::{PlayerSlot, Tick};
    use campfire_runner::SessionWorld;
    use campfire_sim::MatchRate;

    use crate::sim_server::receipts::Receipts;
    use campfire_host::BotDriver;
    use campfire_host::Checkpoints;
    use campfire_host::Fault;
    use campfire_host::PlayerLink;
    use campfire_host::PlayerSlotBot;
    use campfire_host::WorkerFaults;

    /// What a server's world gives the tests and benches of any crate.
    pub trait SimServerWorldInternals {
        /// Makes the server give its receipts in its next update, as if their second passed.
        fn give_receipts(&mut self);

        /// Holds the server's next round of receipts until `give_receipts`, so a test, not the
        /// real clock, says when they go out.
        fn hold_receipts(&mut self);

        /// The player slots of the plan's bots of `player_slots` that play their script from tick
        /// `next` of the world's session on, as a server that starts or restores builds its driver.
        fn planned_bots(&self, player_slots: Vec<PlayerSlotBot>, next: Tick) -> Vec<PlayerSlot>;

        /// Waits for the checkpoint on its thread in the session, whose server keeps its
        /// files, and logs its record; reports the fault when its snapshot was not written.
        fn settle_checkpoint(&mut self);

        /// Makes a checkpoint due at the boundary before `tick` in the session, whose
        /// server keeps its files.
        fn request_checkpoint(&mut self, tick: Tick);

        /// The player slot of the player the client link `link` carries the inputs of; none for a
        /// link of no player.
        fn link_player_slot(&self, link: Entity) -> Option<PlayerSlot>;

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

        fn planned_bots(&self, player_slots: Vec<PlayerSlotBot>, next: Tick) -> Vec<PlayerSlot> {
            let rate = *self.resource::<MatchRate>();
            let planned = BotDriver::planned(player_slots, self.session().log(), next, rate);
            planned.iter().map(|driven| driven.player_slot).collect()
        }

        fn settle_checkpoint(&mut self) {
            if let Err(error) = Checkpoints::settle(self) {
                self.resource_mut::<WorkerFaults>()
                    .report(Fault::Snapshot(error));
            }
        }

        fn request_checkpoint(&mut self, tick: Tick) {
            self.resource_mut::<Checkpoints>().request(tick);
        }

        fn link_player_slot(&self, link: Entity) -> Option<PlayerSlot> {
            self.get::<PlayerLink>(link).map(|link| link.player_slot())
        }

        fn link_refused(&self, link: Entity) -> Option<bool> {
            self.get::<PlayerLink>(link).map(|link| link.is_refused())
        }
    }
}

#[cfg(test)]
mod tests;
