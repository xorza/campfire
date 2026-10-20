use std::fmt;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bevy_app::{App, AppExit, Update};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_common::{ErrorReport, Seat};
use campfire_mode::OptionValue;
use campfire_net_connection::Start;
use campfire_net_endpoint::Endpoint;
use campfire_net_link::Server;
use campfire_net_packets::EmulatedWires;
use campfire_package::ModePackages;
use campfire_sim::MatchRate;
use campfire_store::Worker;
use campfire_visibility::HostVisibility;
use tracing::error;

use crate::hosting::Hosting;
use crate::key_file::KeyFile;
use crate::local::local_link::LocalLink;
use crate::local::local_pace::LocalPacePlugin;
use crate::local::local_server::error::LocalServerError;
use crate::os::Os;
use crate::pace::Pace;
use crate::paced_runner::PacedRunnerPlugin;
use crate::session_times::SessionTimes;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_server::ServerFrameSystems;
use crate::sim_server::lobby::{Lobby, LobbyOpening};
use crate::sim_server::pending_restore::{PendingRestore, UncheckedRestore};
use crate::sim_server::server_bots::{SeatScript, ServerBots};
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::{Publisher, SessionDir};
use crate::sim_server::sim_server_plugins::SimServerPlugins;
use crate::sim_server::sim_server_world::{SessionOpening, SimServerWorld, StopRequest};
use crate::udp_links::{LocalPair, UdpLinks};

pub(crate) mod error;

/// A session's server on a thread of a singleplayer client's process, linked to the client by a
/// sealed link over the emulator: the client's player in seat 0, in each other seat the server's
/// bot its setup names, or none, open for a late join. Its data directory holds what a dedicated
/// server's does; a session an earlier start left ends aborted, its log published, and each start
/// opens a new one. It runs until it drops: it then ends the session, as the mode ended the
/// match or aborted, publishes its log, and its thread ends. Both ends follow the setup's pace.
pub struct LocalServer {
    stop: Arc<AtomicBool>,
    pin: ServerPin,
    /// The client's end of the link, until the client takes it.
    link: Option<LocalLink>,
    relinks: Relinks,
    /// Dropped after the server's `Drop` asks it to stop.
    _worker: Worker,
}

/// The client's end of the new link a local server makes once it starts again after a load,
/// which the client takes; see `LocalRelinkPlugin`.
#[derive(Clone, Default)]
pub struct Relinks(Arc<Mutex<Option<LocalLink>>>);

/// What the local server's thread keeps across the apps it runs: one, then a new one after each
/// load, which restores the session from its data.
#[derive(Debug)]
struct Runs {
    data: PathBuf,
    packages: Arc<ModePackages>,
    bots: ServerBots,
    server: ServerSetup,
    pace: Arc<Pace>,
    tick_hz: NonZeroU32,
    stop: Arc<AtomicBool>,
    /// Set by an app that ends for a load.
    restart: Arc<AtomicBool>,
    relinks: Relinks,
}

/// Set once the app ends for a load, as its thread then starts a new one.
#[derive(Resource, Debug)]
struct Restart(Arc<AtomicBool>);

/// What a local server starts with.
#[derive(Debug)]
pub struct LocalServerSetup {
    pub packages: Arc<ModePackages>,
    /// Its data directory, which it holds locked while it runs.
    pub data: PathBuf,
    /// The bots it plays, none in seat 0.
    pub bots: Vec<SeatScript>,
    /// The values the player picks of the mode's options, the others their defaults.
    pub options: Vec<OptionValue>,
    pub pace: Arc<Pace>,
    pub os: Os,
}

/// Whether the client asked the local server to stop.
#[derive(Resource, Debug)]
struct Stop(Arc<AtomicBool>);

impl LocalServer {
    /// Opens the local server's data directory and session, and starts its thread. An error when
    /// the directory does not open, a session an earlier start left does not end, or the new
    /// one does not open.
    pub fn start(setup: LocalServerSetup) -> Result<Self, LocalServerError> {
        let LocalServerSetup {
            packages,
            data,
            bots,
            options,
            pace,
            os,
        } = setup;
        let dir = ServerDir::open(&data).map_err(LocalServerError::Data)?;
        let signer = KeyFile::read_or_create(&dir.layout().key_file(), os.entropy)
            .map_err(LocalServerError::Key)?;
        let signer = Arc::new(signer);
        let key = signer.public_key();
        let tls_key = UdpLinks::server(&signer)
            .tls_key()
            .expect("a raw key's identity");
        Self::end_earlier(Publisher {
            data: &dir,
            packages: &packages,
            signer: &signer,
        })?;
        let server = ServerSetup {
            signer,
            times: SessionTimes::DEFAULT,
            os,
        };
        let bots = ServerBots::new(bots, None, &packages).map_err(LocalServerError::Bots)?;
        let seats = u32::try_from(packages.manifest().seats()).expect("seats fit u32");
        let open = (1..seats)
            .map(Seat::new)
            .filter(|&seat| bots.seats.iter().all(|bot| bot.seat != seat))
            .collect();
        let tick_hz = packages.manifest().tick_hz.default();
        let opening = LobbyOpening {
            packages: Arc::clone(&packages),
            bots: bots.clone(),
            open,
            host: HostVisibility::default(),
            options,
            server: server.clone(),
        };
        let lobby = Lobby::open(opening, &dir).map_err(LocalServerError::Open)?;
        let LocalPair {
            server: link,
            client,
        } = UdpLinks::local(&server.signer, server.os.entropy);
        let stop = Arc::new(AtomicBool::new(false));
        let relinks = Relinks::default();
        let runs = Runs {
            data,
            packages,
            bots,
            server,
            pace,
            tick_hz,
            stop: Arc::clone(&stop),
            restart: Arc::new(AtomicBool::new(false)),
            relinks: relinks.clone(),
        };
        let opening = SessionOpening::New(Box::new(lobby));
        let session = opening.span();
        let worker = Worker::start("local server", move || {
            let _session = session.entered();
            let mut app = runs.app(link, dir);
            app.world_mut().open_session(opening);
            runs.run(app);
        });
        Ok(Self {
            stop,
            pin: ServerPin {
                key,
                tls_key,
                tick_hz,
            },
            link: Some(LocalLink::new(client)),
            relinks,
            _worker: worker,
        })
    }

    /// Where the client takes the new link the server makes after each load.
    pub fn relinks(&self) -> Relinks {
        self.relinks.clone()
    }

    /// What the client pins: the server's key, its TLS key and its tick rate.
    pub const fn pin(&self) -> ServerPin {
        self.pin
    }

    /// The client's end of the link, once.
    pub const fn take_link(&mut self) -> LocalLink {
        self.link.take().expect("the client takes the link once")
    }

    /// Ends aborted a session an earlier start left under the publisher's data directory, and
    /// publishes its log by `publisher`; removes one whose match never started.
    fn end_earlier(publisher: Publisher<'_>) -> Result<(), LocalServerError> {
        if let Some(session) =
            SessionDir::waiting(publisher.data).map_err(LocalServerError::Waiting)?
        {
            session.abort(publisher).map_err(LocalServerError::Abort)?;
        }
        Ok(())
    }

    /// Ends once the session went back to a save, for its thread to start a new app; else exits
    /// as `SimServerWorld::exit_due` says, the client asking the server to stop.
    fn watch(world: &mut World) {
        if world.is_reload_wanted() {
            world.resource::<Restart>().0.store(true, Ordering::Relaxed);
            world.write_message(AppExit::Success);
            return;
        }
        let stop = if world.resource::<Stop>().0.load(Ordering::Relaxed) {
            StopRequest::Asked
        } else {
            StopRequest::NotAsked
        };
        if let Some(exit) = world.exit_due(stop) {
            world.write_message(exit);
        }
    }
}

impl fmt::Debug for LocalServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalServer")
            .field("stop", &self.stop)
            .field("pin", &self.pin)
            .field("link_taken", &self.link.is_none())
            .finish_non_exhaustive()
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Runs {
    /// Runs `app`, then after each load a new app that restores the session from its data, until
    /// one ends otherwise.
    fn run(self, mut app: App) {
        loop {
            let exit = app.run();
            if !self.restart.swap(false, Ordering::Relaxed) {
                if let AppExit::Error(code) = exit {
                    error!(code = code.get(), "the local server stopped with an error");
                }
                return;
            }
            match self.restored() {
                Ok(next) => app = next,
                Err(error) => {
                    error!(error = %ErrorReport::of(&error), "the local session does not restore after its load");
                    return;
                }
            }
        }
    }

    /// A new app, which restores the session from the data directory in its first frame, the
    /// client's end of its new link given to the client.
    fn restored(&self) -> Result<App, LocalServerError> {
        let dir = ServerDir::open(&self.data).map_err(LocalServerError::Data)?;
        let session = SessionDir::waiting(&dir).map_err(LocalServerError::Waiting)?;
        let LocalPair {
            server: link,
            client,
        } = UdpLinks::local(&self.server.signer, self.server.os.entropy);
        let mut app = self.app(link, dir);
        let restore = PendingRestore::check(UncheckedRestore {
            session: session.expect("the session that loaded, whose match started"),
            packages: Arc::clone(&self.packages),
            server: self.server.clone(),
            bots: self.bots.clone(),
        })
        .expect("the packages that ran the session admit its terms");
        app.world_mut()
            .open_session(SessionOpening::Restored(Box::new(restore)));
        self.relinks.give(LocalLink::new(client));
        Ok(app)
    }

    /// An app of the server: its plugins, its endpoint `link` of the link, started, and its data
    /// directory `dir`, which it holds locked.
    fn app(&self, link: Endpoint<EmulatedWires>, dir: ServerDir) -> App {
        let mut app = App::new();
        app.add_plugins((
            SimServerPlugins {
                tick_hz: self.tick_hz,
                capabilities: self.packages.manifest().capabilities,
                hosting: Hosting::Local,
            },
            PacedRunnerPlugin,
            LocalPacePlugin {
                pace: Arc::clone(&self.pace),
                tick_rate: MatchRate::new(self.tick_hz).rate(),
            },
        ));
        app.insert_resource(dir);
        app.insert_resource(Stop(Arc::clone(&self.stop)));
        app.insert_resource(Restart(Arc::clone(&self.restart)));
        app.add_systems(Update, LocalServer::watch.after(ServerFrameSystems::Saves));
        let server = app.world_mut().spawn((Server::default(), link)).id();
        app.world_mut().trigger(Start { entity: server });
        app
    }
}

impl Relinks {
    fn give(&self, link: LocalLink) {
        *self.lock() = Some(link);
    }

    /// The new link, once the server made one and no client took it yet.
    pub(crate) fn take(&self) -> Option<LocalLink> {
        self.lock().take()
    }

    fn lock(&self) -> MutexGuard<'_, Option<LocalLink>> {
        self.0.lock().expect("no thread panics holding the link")
    }
}

impl fmt::Debug for Relinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Relinks").finish_non_exhaustive()
    }
}
