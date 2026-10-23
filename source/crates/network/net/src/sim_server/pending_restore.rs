use std::sync::Arc;

use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::ErrorReport;
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_runner::{CheckedTerms, Session, SessionRules, TermsError};
use tracing::error;

use crate::sim_server::server_match::{MatchBegin, MatchSource, ServerMatch};
use campfire_host::RestoredSession;
use campfire_host::ServerBots;
use campfire_host::ServerPhase;
use campfire_host::ServerSetup;
use campfire_host::SessionRestored;

/// A session to restore in an app's first frame, as a match starts in a fixed tick: the one a stop
/// ended, as its server starts again, or the one a load went back to. `SimServerPlugin` restores it
/// while the world holds it.
#[derive(Resource, Debug)]
pub struct PendingRestore {
    pub session: RestoredSession,
    pub packages: Arc<ModePackages>,
    pub server: ServerSetup,
    pub bots: ServerBots,
    /// The session's terms as the rules of `packages` checked them.
    checked: CheckedTerms,
}

/// A session to restore, its terms not yet checked by the rules of the packages the server
/// runs now.
#[derive(Debug)]
pub struct UncheckedRestore {
    pub session: RestoredSession,
    pub packages: Arc<ModePackages>,
    pub server: ServerSetup,
    pub bots: ServerBots,
}

impl PendingRestore {
    /// The restore of `unchecked`; an error when the rules of its packages do not admit the
    /// terms the session ran under, as packages changed since then may not.
    pub fn check(unchecked: UncheckedRestore) -> Result<Self, TermsError> {
        let UncheckedRestore {
            session,
            packages,
            server,
            bots,
        } = unchecked;
        let checked = SessionRules::of(&packages).check(session.terms())?;
        Ok(Self {
            session,
            packages,
            server,
            bots,
            checked,
        })
    }

    /// The session's terms as the rules of its packages checked them.
    pub const fn checked(&self) -> &CheckedTerms {
        &self.checked
    }

    /// Restores the session in `world`'s server, its log's bytes from the first frame on, and
    /// logs `SessionRestored`; see `ServerMatch::begin`. An error, logged, ends the app.
    pub fn run(world: &mut World) {
        let Self {
            session,
            packages,
            server,
            bots,
            checked,
        } = world
            .remove_resource::<Self>()
            .expect("a restore runs while one waits");
        let id = session.log.session_id();
        let begin = MatchBegin {
            source: MatchSource::Restored(session),
            packages: &packages,
            server: &server,
            bots,
            checked,
        };
        match ServerMatch::begin(world, begin) {
            Ok(()) => {
                ServerPhase::Running.go(world);
                let tick = world.resource::<Session>().next_tick();
                SessionRestored { session: id, tick }.log();
            }
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the session's match does not restore");
                world.write_message(AppExit::error());
            }
        }
    }
}
