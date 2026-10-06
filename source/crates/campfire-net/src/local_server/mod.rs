use std::fmt;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, TaskPoolPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::system::Commands;
use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{CertificateHash, SeedChain, SessionPrivate};
use campfire_runner::{InputRules, Session};
use campfire_sim::TickRate;
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::server::{RawServer, ServerPlugins};
use lightyear::prelude::{Link, LinkOf, Linked, PeerAddr, ReplicationSender};
use tracing::error;

use crate::data_dir::DataDir;
use crate::events::checkpoint_failed::CheckpointFailed;
use crate::events::journal_failed::JournalFailed;
use crate::events::session_aborted::SessionAborted;
use crate::events::session_written::SessionWritten;
use crate::lobby::{Lobby, LobbySetup};
use crate::local_pace::LocalPace;
use crate::local_server::error::LocalServerError;
use crate::local_session::LocalSession;
use crate::match_clock::MatchClock;
use crate::net_protocol::NetProtocol;
use crate::pace::Pace;
use crate::server_bots::{ServerBots, SlotBot};
use crate::server_setup::ServerSetup;
use crate::session_dir::{RestoredSession, SessionDir};
use crate::session_journal::SessionJournal;
use crate::session_times::SessionTimes;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_server::SimServer;

pub(crate) mod error;

/// In-process channels have no TLS; both ends take this as the certificate's hash.
const CERTIFICATE: CertificateHash = CertificateHash::new([0; 32]);
/// The segments of a local session's seed chain, as a dedicated server's.
const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();
/// How often the server's app loop runs, as a dedicated server's.
const FRAME: Duration = Duration::from_millis(2);
/// The exit code of a server whose journal or snapshot failed, as a dedicated server's.
const JOURNAL_FAILED: u8 = 74;

/// A session's server on a thread of a singleplayer client's process, linked to the client by
/// in-process channels: the client's player in slot 0, in each other slot the server's bot its
/// setup names, or none, open for a late join. Its data directory holds what a dedicated
/// server's does; a session an earlier start left ends aborted, its log published, and each start
/// opens a new one. It runs until it drops: it then ends the session, as the mode ended the
/// match or aborted, publishes its log, and its thread ends. Both ends follow the setup's pace.
pub struct LocalServer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pin: ServerPin,
    /// The client's end of the link, until the client takes it.
    link: Option<CrossbeamIo>,
    relinks: Relinks,
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
    tick: Duration,
    stop: Arc<AtomicBool>,
    /// Set by an app that ends for a load.
    restart: Arc<AtomicBool>,
    relinks: Relinks,
}

/// A session to restore in an app's first frame, after a load.
#[derive(Resource, Debug)]
struct PendingRestore {
    session: RestoredSession,
    packages: Arc<ModePackages>,
    bots: ServerBots,
    server: ServerSetup,
}

/// Set once the app ends for a load, as its thread then starts a new one.
#[derive(Resource, Debug)]
struct Restart(Arc<AtomicBool>);

/// What a local server starts with.
#[derive(Debug)]
pub struct LocalServerSetup {
    pub packages: ModePackages,
    /// Its data directory, which it holds locked while it runs.
    pub data: PathBuf,
    /// The bots it plays, none in slot 0.
    pub bots: Vec<SlotBot>,
    pub pace: Arc<Pace>,
    /// Unix seconds, against which a delegation's expiry is checked.
    pub clock: fn() -> u64,
    /// Fills a key, a seed or BIP-340's auxiliary randomness with random bytes.
    pub entropy: fn(&mut [u8; 32]),
}

/// Whether the client asked the local server to stop.
#[derive(Resource, Debug)]
struct Stop(Arc<AtomicBool>);

/// The local server's data directory, which its app keeps locked, and where the logs go.
#[derive(Resource, Debug)]
struct Data {
    path: PathBuf,
    _dir: DataDir,
}

impl LocalServer {
    /// Opens the local server's data directory and session, and starts its thread. An error when
    /// the directory does not open, a session an earlier start left does not end, or the new
    /// one does not open.
    pub fn start(setup: LocalServerSetup) -> Result<LocalServer, LocalServerError> {
        let LocalServerSetup {
            packages,
            data,
            bots,
            pace,
            clock,
            entropy,
        } = setup;
        assert!(
            bots.iter().all(|bot| bot.slot != PlayerSlot::new(0)),
            "slot 0 is the client's"
        );
        let dir = DataDir::open(&data, entropy).map_err(LocalServerError::Data)?;
        let key = dir.key;
        LocalServer::end_earlier(&data, &packages, key, entropy)?;
        let server = ServerSetup {
            key,
            certificate: CERTIFICATE,
            times: SessionTimes::DEFAULT,
            clock,
            entropy,
        };
        let mut root = [0; 32];
        entropy(&mut root);
        let seed_chain = SeedChain::new(root, SEGMENTS);
        let slots = u32::try_from(packages.manifest().slots()).expect("slots fit u32");
        let open = (1..slots)
            .map(PlayerSlot::new)
            .filter(|&slot| bots.iter().all(|bot| bot.slot != slot))
            .collect();
        let tick_hz = packages.manifest().tick_hz.default();
        let packages = Arc::new(packages);
        let bots = ServerBots {
            slots: bots,
            takeover: None,
        };
        let mut lobby = Lobby::new(LobbySetup {
            packages: Arc::clone(&packages),
            seed_chain,
            tick_hz,
            inputs: InputRules::LAN,
            slots: usize::try_from(slots).expect("slots fit usize"),
            bots: bots.clone(),
            open,
            server,
        })
        .map_err(LocalServerError::Terms)?;
        let private = SessionPrivate {
            seed_chain,
            terms: lobby.terms().clone(),
        };
        let session = SessionDir::create(&data, &private).map_err(LocalServerError::NewSession)?;
        lobby.keep_files(session.start().map_err(LocalServerError::NewJournal)?);
        let (client_io, server_io) = CrossbeamIo::new_pair();
        let stop = Arc::new(AtomicBool::new(false));
        let relinks = Relinks::default();
        let runs = Runs {
            data,
            packages,
            bots,
            server,
            pace,
            tick: TickRate::new(tick_hz).length(),
            stop: Arc::clone(&stop),
            restart: Arc::new(AtomicBool::new(false)),
            relinks: relinks.clone(),
        };
        let thread = thread::Builder::new()
            .name("local server".to_owned())
            .spawn(move || {
                let mut app = runs.app(server_io, dir);
                app.insert_resource(lobby);
                runs.run(app);
            })
            .expect("the OS starts a thread");
        Ok(LocalServer {
            stop,
            thread: Some(thread),
            pin: ServerPin {
                key: key.x_only_public_key().0,
                certificate: CERTIFICATE,
                tick_hz,
            },
            link: Some(client_io),
            relinks,
        })
    }

    /// Where the client takes the new link the server makes after each load.
    pub fn relinks(&self) -> Relinks {
        self.relinks.clone()
    }

    /// What the client pins: the server's key, its certificate's hash and its tick rate.
    pub const fn pin(&self) -> ServerPin {
        self.pin
    }

    /// The client's end of the link, once.
    pub fn take_link(&mut self) -> CrossbeamIo {
        self.link.take().expect("the client takes the link once")
    }

    /// Ends aborted a session an earlier start left under `data`, of the mode `packages` holds,
    /// and publishes its log; removes one whose match never started.
    fn end_earlier(
        data: &Path,
        packages: &ModePackages,
        key: Keypair,
        entropy: fn(&mut [u8; 32]),
    ) -> Result<(), LocalServerError> {
        let Some(dir) = SessionDir::find(data).map_err(LocalServerError::Find)? else {
            return Ok(());
        };
        let Some(session) = dir.restore().map_err(LocalServerError::Restore)? else {
            return dir.remove().map_err(LocalServerError::Remove);
        };
        let id = session.log.session_id();
        let file = session
            .abort(data, packages, key, entropy)
            .map_err(LocalServerError::Abort)?;
        SessionAborted { session: id, file }.log();
        Ok(())
    }

    /// Restores the session that waits, after a load; an error, logged, ends the app.
    fn restore(world: &mut World) {
        let PendingRestore {
            session,
            packages,
            bots,
            server,
        } = world
            .remove_resource::<PendingRestore>()
            .expect("a restore runs while one waits");
        if let Err(error) = SimServer::restore_match(world, session, &packages, &server, bots) {
            error!(%error, "the local session does not restore after its load");
            world.write_message(AppExit::error());
        }
    }

    /// Ends once the session went back to a save, for its thread to start a new app; exits once
    /// a write of the journal or a snapshot failed; ends the session once the client
    /// asked the server to stop, or every player left, as the mode ended the match or aborted;
    /// publishes the log of the session once it ended, and exits; exits at once when asked to
    /// stop before the match started.
    fn watch(world: &mut World) {
        if SimServer::reload_wanted(world) {
            world.resource::<Restart>().0.store(true, Ordering::Relaxed);
            world.write_message(AppExit::Success);
            return;
        }
        if let Some(failure) = world
            .get_resource::<SessionJournal>()
            .and_then(|journal| journal.0.take_failure())
        {
            JournalFailed {
                error: failure.to_string(),
            }
            .log();
            world.write_message(AppExit::from_code(JOURNAL_FAILED));
            return;
        }
        if let Some(error) = SimServer::checkpoint_failure(world) {
            CheckpointFailed {
                error: error.to_string(),
            }
            .log();
            world.write_message(AppExit::from_code(JOURNAL_FAILED));
            return;
        }
        let stop = world.resource::<Stop>().0.load(Ordering::Relaxed);
        if !world.contains_resource::<MatchClock>() {
            if stop {
                world.write_message(AppExit::Success);
            }
            return;
        }
        if world.resource::<Session>().log().result().is_none() {
            if !stop && !SimServer::players_gone(world) {
                return;
            }
            let outcome = Session::outcome(world);
            if let Err(error) = SimServer::end_session(world, outcome) {
                CheckpointFailed {
                    error: error.to_string(),
                }
                .log();
                world.write_message(AppExit::from_code(JOURNAL_FAILED));
                return;
            }
        }
        let session = world.resource::<Session>();
        let id = session.log().session_id();
        let hash = session.state_hash(world);
        let exit = match SessionDir::publish(&world.resource::<Data>().path, session.log()) {
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
                error!(session = %id, %error, "could not write the session log");
                AppExit::error()
            }
        };
        world.write_message(exit);
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
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the local server does not panic");
        }
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
                    error!(%error, "the local session does not restore after its load");
                    return;
                }
            }
        }
    }

    /// A new app, which restores the session from the data directory in its first frame, the
    /// client's end of its new link given to the client.
    fn restored(&self) -> Result<App, LocalServerError> {
        let dir = DataDir::open(&self.data, self.server.entropy).map_err(LocalServerError::Data)?;
        let found = SessionDir::find(&self.data).map_err(LocalServerError::Find)?;
        let found = found.expect("the session that loaded");
        let session = found.restore().map_err(LocalServerError::Restore)?;
        let (client_io, server_io) = CrossbeamIo::new_pair();
        let mut app = self.app(server_io, dir);
        app.insert_resource(PendingRestore {
            session: session.expect("a session whose match started"),
            packages: Arc::clone(&self.packages),
            bots: self.bots.clone(),
            server: self.server,
        });
        self.relinks.give(client_io);
        Ok(app)
    }

    /// An app of the server: its plugins, its end of the link `link`, and its data directory
    /// `dir`, which it holds locked.
    fn app(&self, link: CrossbeamIo, dir: DataDir) -> App {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            StatesPlugin,
            ScheduleRunnerPlugin::run_loop(FRAME),
        ));
        app.add_plugins(ServerPlugins {
            tick_duration: self.tick,
        });
        app.add_plugins((
            NetProtocol,
            SimServer,
            LocalPace {
                pace: Arc::clone(&self.pace),
                tick: self.tick,
            },
        ));
        app.add_observer(
            |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
                commands.entity(added.entity).insert(ReplicationSender);
            },
        );
        app.insert_resource(LocalSession);
        app.insert_resource(Data {
            path: self.data.clone(),
            _dir: dir,
        });
        app.insert_resource(Stop(Arc::clone(&self.stop)));
        app.insert_resource(Restart(Arc::clone(&self.restart)));
        app.add_systems(
            Update,
            (
                LocalServer::restore.run_if(resource_exists::<PendingRestore>),
                LocalServer::watch,
            )
                .chain(),
        );
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server = app.world_mut().spawn((RawServer, Linked)).id();
        let address = SocketAddr::from(([127, 0, 0, 1], 1));
        app.world_mut().spawn((
            LinkOf { server },
            Link::default(),
            PeerAddr(address),
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
