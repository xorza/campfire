use bevy_app::AppExit;
use bevy_ecs::world::World;
use campfire_log::{ErrorReport, LogEvent};
use campfire_runner::Session;
use tracing::error;

use crate::events::session_written::SessionWritten;
use crate::faults::Faults;
use crate::faults::fault::Fault;
use crate::match_clock::MatchClock;
use crate::sim_server::SimServer;
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::session_dir::SessionDir;

/// When a session's server exits, a dedicated one or a local one.
#[derive(Debug)]
pub struct ServerExit;

impl ServerExit {
    /// How the server of the session in `world` exits in this frame, when it does: as `Faults`
    /// says, once a worker failed, which for its journal or a checkpoint's snapshot is with
    /// `ExitStatus::Storage`; before the match started, at once when `stop` asks it to. Once the
    /// match started, when `stop` asks it to or every player left, it ends the session, as the
    /// mode ended the match or aborted; it then publishes the log in its `ServerDir`, and exits,
    /// with an error when the log is not written, as the session it holds is lost.
    pub fn due(world: &mut World, stop: bool) -> Option<AppExit> {
        if let Some(exit) = Faults::apply(world) {
            return Some(exit);
        }
        if !world.contains_resource::<MatchClock>() {
            return stop.then_some(AppExit::Success);
        }
        if world.resource::<Session>().log().result().is_none() {
            if !stop && !SimServer::players_gone(world) {
                return None;
            }
            let outcome = Session::outcome(world);
            if let Err(error) = SimServer::end_session(world, outcome) {
                world
                    .resource_mut::<Faults>()
                    .report(Fault::Snapshot(error));
                return Faults::apply(world);
            }
        }
        let session = world.resource::<Session>();
        let id = session.log().session_id();
        let hash = session.state_hash(world);
        let data = world.resource::<ServerDir>();
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
