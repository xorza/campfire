use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bevy_app::{App, AppExit, Update};
use bevy_ecs::bundle::Bundle;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::ExitStatus;
use campfire_log::LogEvent;
use campfire_net_connection::Start;
use campfire_net_link::{Linked, Server};
use campfire_platform::StopSignals;
use campfire_sim::MatchRate;

use crate::local_pace::LocalPacePlugin;
use crate::paced_runner::PacedRunnerPlugin;
use crate::sim_server::ServerFrameSystems;
use crate::sim_server::sim_server_plugins::SimServerPlugins;
use crate::sim_server::sim_server_world::{SessionOpening, SimServerWorld, StopRequest};
use campfire_host::ClientLayout;
use campfire_host::Hosting;
use campfire_host::Listening;
use campfire_host::Pace;
use campfire_host::Reports;
use campfire_host::ServerDir;
use campfire_host::{ConsoleStdio, LogSeen, MatchConsole};

/// What a server's app is built from: the session it opens, its data directory, which it holds
/// locked, its end `endpoint` of its transport, which starts with the app, and how it runs.
#[derive(Debug)]
pub struct ServerAppSetup<E> {
    pub opening: SessionOpening,
    pub data: ServerDir,
    pub endpoint: E,
    pub run: ServerRun,
}

/// How a server runs and stops: in a process of its own, or on a thread of its client's.
#[derive(Debug)]
pub enum ServerRun {
    /// It logs `listening` once its transport listens, and exits once its session ends, or at a
    /// signal `stop` caught or the end of its console's input, with its code, the session left in
    /// its journal for the next start to restore (08-sessions.md, A stop is an exit, not a
    /// crash; console.md, A game stops as its console's input ends). Its `console`, when it has
    /// one, answers between its ticks.
    Dedicated {
        stop: StopSignals,
        listening: Listening,
        console: Option<Box<ConsoleStdio>>,
    },
    /// It follows its client's `pace`, exits once its session ends, as its client asks by
    /// `stop`, and writes its log into its client's reports, of the data directory `reports`
    /// (08-sessions.md, The log is the local server's, published at the report's tick). With a
    /// console, its client's, which `seen` the tap of, it answers the requests its client
    /// forwards, and holds the match paused after its first tick (console.md, A console's local
    /// match starts paused).
    Local {
        stop: Arc<AtomicBool>,
        pace: Arc<Pace>,
        reports: ClientLayout,
        console: Option<Arc<Mutex<LogSeen>>>,
    },
}

/// Whether a local server's client asked it to stop.
#[derive(Resource, Debug)]
struct LocalStop(Arc<AtomicBool>);

impl<E: Bundle> ServerAppSetup<E> {
    /// The server's app: its plugins at the rate of its session, from the session's packages,
    /// that session open, and its server started on its endpoint.
    pub fn build(self) -> App {
        let Self {
            opening,
            data,
            endpoint,
            run,
        } = self;
        let tick_hz = opening.terms().tick_hz;
        let hosting = match run {
            ServerRun::Dedicated { .. } => Hosting::Remote,
            ServerRun::Local { .. } => Hosting::Local,
        };
        let mut app = App::new();
        app.add_plugins((
            SimServerPlugins {
                tick_hz,
                capabilities: opening.packages().manifest().capabilities,
                hosting,
            },
            PacedRunnerPlugin,
        ));
        match run {
            ServerRun::Dedicated {
                stop,
                listening,
                console,
            } => {
                app.add_observer(
                    move |added: On<'_, '_, Add<Linked>>,
                          servers: Query<'_, '_, (), With<Server>>| {
                        if servers.contains(added.entity) {
                            listening.log();
                        }
                    },
                );
                if let Some(stdio) = console {
                    app.insert_resource(MatchConsole::new(None, Arc::clone(stdio.seen())));
                    app.insert_resource(*stdio);
                }
                app.add_systems(
                    Update,
                    (ServerRun::exit_due, move |world: &mut World| {
                        let ended = world
                            .get_resource::<ConsoleStdio>()
                            .is_some_and(ConsoleStdio::is_ended);
                        if stop.is_received() || ended {
                            world.write_message(AppExit::from_code(ExitStatus::Interrupted.code()));
                        }
                    })
                        .chain()
                        .after(ServerFrameSystems::Console),
                );
            }
            ServerRun::Local {
                stop,
                pace,
                reports,
                console,
            } => {
                if let Some(seen) = console {
                    let console = MatchConsole::new(Some(Arc::clone(&pace)), seen);
                    console.hold_first_tick();
                    app.insert_resource(console);
                }
                app.add_plugins(LocalPacePlugin {
                    pace,
                    tick_rate: MatchRate::new(tick_hz).rate(),
                });
                app.insert_resource(LocalStop(stop));
                app.insert_resource(Reports::new(reports));
                app.add_systems(
                    Update,
                    ServerRun::exit_local.after(ServerFrameSystems::Console),
                );
            }
        }
        app.world_mut().open_session(opening);
        app.insert_resource(data);
        let server = app.world_mut().spawn((Server::default(), endpoint)).id();
        app.world_mut().trigger(Start { entity: server });
        app
    }
}

impl ServerRun {
    /// Exits as `SimServerWorld::exit_due` says: a dedicated server stops only once its session
    /// ended.
    fn exit_due(world: &mut World) {
        if let Some(exit) = world.exit_due(StopRequest::NotAsked) {
            world.write_message(exit);
        }
    }

    /// Exits as `SimServerWorld::exit_due` says, the client asking the server to stop.
    fn exit_local(world: &mut World) {
        let stop = if world.resource::<LocalStop>().0.load(Ordering::Relaxed) {
            StopRequest::Asked
        } else {
            StopRequest::NotAsked
        };
        if let Some(exit) = world.exit_due(stop) {
            world.write_message(exit);
        }
    }
}
