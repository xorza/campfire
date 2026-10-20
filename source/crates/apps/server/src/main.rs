//! Headless game server: opens a session of a mode on an address, lets its players join over
//! the stack's UDP, runs the match, and writes the session log once every player left. Its data
//! directory, which it holds locked while it runs, keeps its key. Ctrl-C, `SIGTERM` or `SIGHUP`,
//! each unless its start ignored it, stops it by an exit with code 130: its links close and its
//! port mapping goes, and its session waits in the journal for the next start to restore; a
//! second signal ends it at once.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info` by default). With `CAMPFIRE_LOG` set
//! to a path, they also go there as JSON lines, filtered by `CAMPFIRE_LOG_FILTER` (Campfire's
//! `debug` by default) with the directives of `CAMPFIRE_LOG_FILTER_EXTRA` added.

use std::env;
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::SystemTime;

use bevy_app::{App, AppExit, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::ErrorReport;
use campfire_common::{ExitStatus, TlsKey};
use campfire_declarations::CapabilitySet;
use campfire_log::{LogEvent, Logging};
use campfire_net::{
    Hosting, JournalFailed, KeyFile, Listening, LobbyOpenError, Os, PacedRunnerPlugin, Publisher,
    RestoredSession, ServerBots, ServerDir, ServerFrameSystems, ServerSetup, SessionOpening,
    SimServerPlugins, SimServerWorld, StopRequest, UdpLinks, exit_code,
};
use campfire_net_connection::Start;
use campfire_net_endpoint::Endpoint;
use campfire_net_link::{Linked, Server};
use campfire_net_port_map::PortMap;
use campfire_net_socket::ServerSocket;
use campfire_package_disk::PackageDir;
use campfire_store::StopSignals;
use tracing::{error, info};

use crate::command_line::{Args, CommandLine};
use crate::error::LaunchError;
use crate::opening::error::OpeningError;
use crate::opening::{Opening, OpeningSetup, RestoreWindow};

mod command_line;
mod error;
mod opening;

/// What the terminal shows when `RUST_LOG` does not say.
const TERMINAL_FILTER: &str = "info";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and the stack's rollbacks; of the rest, `info` and above.
const FILE_FILTER: &str = "info,campfire_server=debug,campfire_net=debug,campfire_runner=debug,\
                           campfire_script=debug,campfire_net_prediction=debug";

fn main() -> ExitCode {
    let _log = Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    match run() {
        Ok(code) => code,
        Err(error) => {
            refused(&error);
            ExitCode::from(error.status())
        }
    }
}

/// Starts the server and runs it: the exit code of its end, or the refusal that ended its start.
fn run() -> Result<ExitCode, LaunchError> {
    // Caught first, so a signal during the start stops the server at its first frame.
    let stop = StopSignals::catch()?;
    let line =
        Logging::command_line::<CommandLine>(env::args_os()).map_err(LaunchError::CommandLine)?;
    let args = Args::from(line);
    let scripts = args.bot_scripts()?;
    let Args {
        data,
        times,
        mode,
        map,
        host,
        options,
        seat_picks,
        address,
        port_map,
        ..
    } = args;
    let data = ServerDir::open(&data)?;
    let signer = Arc::new(KeyFile::read_or_create(
        &data.layout().key_file(),
        Os::SYSTEM.entropy,
    )?);
    // The socket binds before any session opens, so a port in use ends the start and leaves the
    // data as it was.
    let socket = ServerSocket::bind(address, UdpLinks::server(&signer))?;
    let tls_key = socket.tls_key().expect("a raw key's identity");
    let no_session = |error| LaunchError::Session {
        data: data.layout().path().to_owned(),
        error,
    };
    // A waiting session names its own map, which its restore, or its end, replays.
    let waiting = Opening::waiting(&data).map_err(no_session)?;
    let journal = waiting.as_ref().map(|session| &session.terms().map);
    let map = Opening::map(journal, map).map_err(no_session)?;
    let packages = PackageDir::new(&mode).mode_packages(map.as_ref())?;
    let bots = ServerBots::new(scripts.seats, scripts.takeover.as_ref(), &packages)?;
    let capabilities = packages.manifest().capabilities;
    let setup = OpeningSetup {
        data: &data,
        packages,
        server: ServerSetup {
            signer,
            times,
            os: Os::SYSTEM,
        },
        bots,
        host,
        options,
        seat_picks,
    };
    let opening = open(waiting, setup).map_err(no_session)?;
    let local = socket.local_address();
    let admission = UdpLinks::admission(opening.seats(), opening.server().times.grace);
    let listening = announce(&opening, &mode, local, tls_key);
    let session = opening.session.span();
    let mut app = server_app(opening, data, listening, capabilities, stop);
    let port_map = port_map
        .then(|| PortMap::start(local, "campfire"))
        .flatten();
    let endpoint = Endpoint::from(socket.serve(admission));
    let server = app.world_mut().spawn((Server::default(), endpoint)).id();
    app.world_mut().trigger(Start { entity: server });
    let exit = session.in_scope(|| app.run());
    // The endpoint closes each link before the mapping goes, whose delete can wait a UPnP request's
    // 10 s, which no client waits for (11-net-stack/port-map.md, A server maps its own port).
    drop(app);
    drop(port_map);
    Ok(exit_code(exit))
}

/// Logs `error`, the refusal that ended the start: a journal that keeps no files as the event a
/// check reads, a command line not at all, as its parser printed it.
fn refused(error: &LaunchError) {
    match error {
        LaunchError::CommandLine(_) => {}
        LaunchError::Session {
            error: error @ OpeningError::Open(LobbyOpenError::KeepFiles(_)),
            ..
        } => JournalFailed {
            error: ErrorReport::of(error).to_string(),
        }
        .log(),
        error => error!(error = %ErrorReport::of(error), "the server does not start"),
    }
}

/// The session `waiting` in the data directory to restore, or a new one of the mode `setup`
/// holds, with its bots, its visibility and its server.
fn open(
    waiting: Option<RestoredSession>,
    setup: OpeningSetup<'_>,
) -> Result<Opening, OpeningError> {
    let publisher = Publisher {
        data: setup.data,
        packages: &setup.packages,
        signer: &setup.server.signer,
    };
    let window = RestoreWindow {
        length: setup.server.times.restore_window,
        now: SystemTime::now(),
    };
    let found = Opening::find(waiting, window, publisher)?;
    Opening::of(found, setup)
}

/// The server's app: its plugins at the rate of the session of `opening`, that session, a
/// `listening` event logged once its transport listens, and the end once every player left, or at
/// a signal `stop` caught, with its code, the session left in its journal for the next start to
/// restore (08-sessions.md, A stop is an exit, not a crash).
fn server_app(
    opening: Opening,
    data: ServerDir,
    listening: Listening,
    set: CapabilitySet,
    stop: StopSignals,
) -> App {
    let mut app = App::new();
    app.add_plugins((
        SimServerPlugins {
            tick_hz: opening.terms().tick_hz,
            capabilities: set,
            hosting: Hosting::Remote,
        },
        PacedRunnerPlugin,
    ));
    app.world_mut().open_session(opening.session);
    app.insert_resource(data);
    app.add_observer(
        move |added: On<'_, '_, Add<Linked>>, servers: Query<'_, '_, (), With<Server>>| {
            if servers.contains(added.entity) {
                listening.log();
            }
        },
    );
    app.add_systems(
        Update,
        (exit_when_due, move |world: &mut World| {
            if stop.is_received() {
                world.write_message(AppExit::from_code(ExitStatus::Interrupted.code()));
            }
        })
            .chain()
            .after(ServerFrameSystems::Saves),
    );
    app
}

/// What the server logs once its transport listens for the session `opening` starts: how a
/// player joins it, through its socket's address `address` and the TLS key `tls_key`; and logs
/// now, for a new session, that it opened, of the mode `mode`.
fn announce(opening: &Opening, mode: &Path, address: SocketAddr, tls_key: TlsKey) -> Listening {
    let terms = opening.terms();
    if let SessionOpening::New(_) = opening.session {
        info!(
            session = %terms.session_id(),
            %address,
            players = terms.seats.len(),
            mode = %mode.display(),
            "opened a session"
        );
    }
    let key = terms.server_key;
    let tick_hz = terms.tick_hz;
    Listening {
        address,
        tls_key,
        server_key: key,
        tick_hz,
        join: format!(
            "campfire-client {} <this machine's LAN address>:{} {tls_key} {key} {tick_hz}",
            mode.display(),
            address.port()
        ),
    }
}

/// Exits as `SimServerWorld::exit_due` says: the server stops only once its session ended.
fn exit_when_due(world: &mut World) {
    if let Some(exit) = world.exit_due(StopRequest::NotAsked) {
        world.write_message(exit);
    }
}
