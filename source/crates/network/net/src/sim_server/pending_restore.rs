use std::sync::Arc;

use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_log::{ErrorReport, LogEvent};
use campfire_package::ModePackages;
use campfire_runner::Session;
use tracing::error;

use crate::events::session_restored::SessionRestored;
use crate::sim_server::SimServer;
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::RestoredSession;

/// A session to restore in an app's first frame, as a match starts in a fixed tick: the one a
/// stop ended, as its server starts again, or the one a load went back to. `SimServer` restores
/// it while the world holds it.
#[derive(Resource, Debug)]
pub struct PendingRestore {
    pub session: RestoredSession,
    pub packages: Arc<ModePackages>,
    pub server: ServerSetup,
    pub bots: ServerBots,
}

impl PendingRestore {
    /// Restores the session in `world`'s server, its log's bytes from the first frame on, and
    /// logs `SessionRestored`; see `SimServer::restore_match`. An error, logged, ends the app.
    pub fn run(world: &mut World) {
        let PendingRestore {
            session,
            packages,
            server,
            bots,
        } = world
            .remove_resource::<PendingRestore>()
            .expect("a restore runs while one waits");
        let id = session.log.session_id();
        match SimServer::restore_match(world, session, &packages, &server, bots) {
            Ok(()) => {
                let tick = world.resource::<Session>().log().next_tick();
                SessionRestored { session: id, tick }.log();
            }
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the session's match does not restore");
                world.write_message(AppExit::error());
            }
        }
    }
}
