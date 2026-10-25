use std::sync::Arc;

use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::{ErrorReport, Tick};
use campfire_host::{Pace, ServerSetup};
use campfire_package::ModePackages;
use campfire_protocol::{ServerSeeds, SessionLog};
use campfire_runner::{CheckedTerms, Session};
use tracing::{error, info};

use crate::sim_server::server_match::{ReplayBegin, ServerMatch};

/// A published log to replay in an app's first frame, as a local server opens a player's report
/// (08-sessions.md, The client opens a report): the log, its seeds, which it reveals, the tick
/// the replay runs through before the match's clock starts, the log's mode, the server's setup,
/// the log's terms as the mode's rules checked them, and the pace the replay holds at its end.
/// `SimServerPlugin` replays it while the world holds it.
#[derive(Resource, Debug)]
pub struct PendingReplay {
    pub log: SessionLog,
    pub seeds: ServerSeeds,
    pub through: Tick,
    pub packages: Arc<ModePackages>,
    pub server: ServerSetup,
    pub checked: CheckedTerms,
    pub pace: Arc<Pace>,
}

/// The pace of a replay, which it holds paused once its log ended, and whether it told so.
#[derive(Resource, Debug)]
pub(crate) struct ReplayEnd {
    pace: Arc<Pace>,
    told: bool,
}

impl PendingReplay {
    /// Replays the log in `world`'s server through its tick, as `ServerMatch::begin_replay`
    /// does, and logs it; an error, logged, ends the app.
    pub(crate) fn run(world: &mut World) {
        let Self {
            log,
            seeds,
            through,
            packages,
            server,
            checked,
            pace,
        } = world
            .remove_resource::<Self>()
            .expect("a replay runs while one waits");
        let id = log.session_id();
        let begin = ReplayBegin {
            log,
            seeds,
            through,
            packages: &packages,
            server: &server,
            checked,
        };
        match ServerMatch::begin_replay(world, begin) {
            Ok(()) => {
                world.insert_resource(ReplayEnd { pace, told: false });
                info!(session = %id, tick = %through, "replayed the log through the report's tick");
            }
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the log's match does not replay");
                world.write_message(AppExit::error());
            }
        }
    }
}

impl ReplayEnd {
    /// Holds the pace paused once the session ran the log's last tick, as there is no tick
    /// more to play, so the replay's viewer stands at the log's end as at the report's tick;
    /// logs it once.
    pub(crate) fn hold(world: &mut World) {
        let session = world.resource::<Session>();
        if session.runs_on() {
            return;
        }
        let tick = session.next_tick();
        let mut end = world.resource_mut::<Self>();
        end.pace.set_paused(true);
        if !end.told {
            end.told = true;
            info!(%tick, "the replay ran its log's last tick");
        }
    }
}
