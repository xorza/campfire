use std::fmt;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, Update};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_common::ErrorReport;
use campfire_common::{ChannelBinding, PlayerSlot, TlsKey};
use campfire_mode::OptionValue;
use campfire_package::ModePackages;
use campfire_quic::{LinkBinding, QuicEndpoint};
use campfire_sim::TickRate;
use campfire_store::Worker;
use campfire_visibility::HostVisibility;
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::server::RawServer;
use lightyear::prelude::{LinkOf, Linked, PeerAddr};
use tracing::error;

use crate::hosting::Hosting;
use crate::key_file::KeyFile;
use crate::local::local_link::LocalLink;
use crate::local::local_pace::LocalPace;
use crate::local::local_server::error::LocalServerError;
use crate::net_protocol::NetProtocol;
use crate::os::Os;
use crate::pace::Pace;
use crate::session_times::SessionTimes;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_server::ServerFrameSystems;
use crate::sim_server::lobby::{Lobby, LobbyOpening};
use crate::sim_server::pending_restore::{PendingRestore, UncheckedRestore};
use crate::sim_server::server_bots::{ServerBots, SlotScript};
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::{Publisher, SessionDir};
use crate::sim_server::sim_server_plugins::SimServerPlugins;
use crate::sim_server::sim_server_world::{SessionOpening, SimServerWorld, StopRequest};

pub(crate) mod error;

/// In-process channels have no TLS; the client pins this as the server's TLS key.
const TLS_KEY: TlsKey = TlsKey::new([0; 32]);

/// A session's server on a thread of a singleplayer client's process, linked to the client by
/// in-process channels: the client's player in slot 0, in each other slot the server's bot its
/// setup names, or none, open for a late join. Its data directory holds what a dedicated
/// server's does; a session an earlier start left ends aborted, its log published, and each start
/// opens a new one. It runs until it drops: it then ends the session, as the mode ended the
/// match or aborted, publishes its log, and its thread ends. Both ends follow the setup's pace.
pub struct LocalServer {
    stop: Arc<AtomicBool>,
    pin: ServerPin,
    /// The client's end of the link, until the client takes it.
    link: Option<CrossbeamIo>,
    relinks: Relinks,
    /// Dropped after the server's `Drop` asks it to stop.
    _worker: Worker,
}

/// The client's end of the new link a local server makes once it starts again after a load,
/// which the client takes; see `LocalRelink`.
#[derive(Clone, Default)]
pub struct Relinks(Arc<Mutex<Option<CrossbeamIo>>>);

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
    /// The bots it plays, none in slot 0.
    pub bots: Vec<SlotScript>,
    /// The values the player picks of the mode's options, the others their defaults.
    pub options: Vec<OptionValue>,
    pub pace: Arc<Pace>,
    pub os: Os,
}

/// Whether the client asked the local server to stop.
#[derive(Resource, Debug)]
struct Stop(Arc<AtomicBool>);

impl LocalServer {
    /// In-process channels have no TLS session: both ends of a local link take this binding
    /// (15-quic-transport.md, The answer binds to the TLS session).
    const BINDING: ChannelBinding = ChannelBinding::new([0; 32]);

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
        let slots = u32::try_from(packages.manifest().slots()).expect("slots fit u32");
        let open = (1..slots)
            .map(PlayerSlot::new)
            .filter(|&slot| bots.slots.iter().all(|bot| bot.slot != slot))
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
        let (client_io, server_io) = CrossbeamIo::new_pair();
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
            let mut app = runs.app(server_io, dir);
            app.world_mut().open_session(opening);
            runs.run(app);
        });
        Ok(Self {
            stop,
            pin: ServerPin {
                key,
                tls_key: TLS_KEY,
                tick_hz,
            },
            link: Some(client_io),
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

    /// The client's end of the link, once, with the binding both ends take.
    pub const fn take_link(&mut self) -> LocalLink {
        LocalLink::new(
            self.link.take().expect("the client takes the link once"),
            LinkBinding(Self::BINDING),
        )
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
        if world.reload_wanted() {
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
        let (client_io, server_io) = CrossbeamIo::new_pair();
        let mut app = self.app(server_io, dir);
        let restore = PendingRestore::check(UncheckedRestore {
            session: session.expect("the session that loaded, whose match started"),
            packages: Arc::clone(&self.packages),
            server: self.server.clone(),
            bots: self.bots.clone(),
        })
        .expect("the packages that ran the session admit its terms");
        app.world_mut()
            .open_session(SessionOpening::Restored(Box::new(restore)));
        self.relinks.give(client_io);
        Ok(app)
    }

    /// An app of the server: its plugins, its end of the link `link`, and its data directory
    /// `dir`, which it holds locked.
    fn app(&self, link: CrossbeamIo, dir: ServerDir) -> App {
        let mut app = App::new();
        app.add_plugins((
            SimServerPlugins {
                tick_hz: self.tick_hz,
                capabilities: self.packages.manifest().capabilities,
                hosting: Hosting::Local,
            },
            ScheduleRunnerPlugin::run_loop(NetProtocol::FRAME),
            LocalPace {
                pace: Arc::clone(&self.pace),
                tick: TickRate::new(self.tick_hz).length(),
            },
        ));
        app.insert_resource(dir);
        app.insert_resource(Stop(Arc::clone(&self.stop)));
        app.insert_resource(Restart(Arc::clone(&self.restart)));
        app.add_systems(Update, LocalServer::watch.after(ServerFrameSystems::Saves));
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server = app.world_mut().spawn((RawServer, Linked)).id();
        let address = SocketAddr::from(([127, 0, 0, 1], 1));
        app.world_mut().spawn((
            LinkOf { server },
            QuicEndpoint::link(),
            PeerAddr(address),
            LinkBinding(LocalServer::BINDING),
            Linked,
            link,
        ));
        app
    }
}

impl Relinks {
    fn give(&self, link: CrossbeamIo) {
        *self.lock() = Some(link);
    }

    /// The new link, once the server made one and no client took it yet.
    pub(crate) fn take(&self) -> Option<CrossbeamIo> {
        self.lock().take()
    }

    fn lock(&self) -> MutexGuard<'_, Option<CrossbeamIo>> {
        self.0.lock().expect("no thread panics holding the link")
    }
}

impl fmt::Debug for Relinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Relinks").finish_non_exhaustive()
    }
}
