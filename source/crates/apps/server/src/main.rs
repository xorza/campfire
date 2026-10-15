//! Headless game server: opens a session of a mode on an address, lets its players join over
//! QUIC, runs the match, and writes the session log once every player left. Its data
//! directory, which it holds locked while it runs, keeps its key. Ctrl-C, `SIGTERM` or `SIGHUP`,
//! each unless its start ignored it, stops it by an exit with code 130: its links close and its
//! port mapping goes, and its session waits in the journal for the next start to restore; a
//! second signal ends it at once.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info` by default). With `CAMPFIRE_LOG` set
//! to a path, they also go there as JSON lines, filtered by `CAMPFIRE_LOG_FILTER` (Campfire's
//! `debug` by default) with the directives of `CAMPFIRE_LOG_FILTER_EXTRA` added.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::Path;
use std::process::ExitCode;

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::{ExitStatus, TlsKey};
use campfire_declare::CapabilitySet;
use campfire_log::{ErrorReport, LogEvent, Logging};
use campfire_net::{
    JournalFailed, KeepFilesError, KeyFile, Listening, NetProtocol, Os, PendingRestore,
    ProcessExit, RestoredSession, ServerBots, ServerDir, ServerSetup, SessionTimes,
    SimServerPlugins, SimServerWorld,
};
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_quic::{AddressLimit, LinkTimes, PortMap, QuicEndpoint};
use campfire_store::StopSignals;
use campfire_visibility::HostVisibility;
use lightyear::prelude::Linked;
use lightyear::prelude::server::{RawServer, Start};
use secp256k1::Keypair;
use tracing::{error, info};
use zeroize::Zeroizing;

use crate::args::Args;
use crate::opening::error::OpeningError;
use crate::opening::{Opened, Opening, OpeningSetup};

mod args;
mod opening;

/// The segments of a session's seed chain: the most checkpoints it may take, less one.
const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

/// What the terminal shows when `RUST_LOG` does not say.
const TERMINAL_FILTER: &str = "info";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and Lightyear's rollbacks; of the rest, `info` and above.
const FILE_FILTER: &str = "info,campfire_server=debug,campfire_net=debug,campfire_runner=debug,\
                           campfire_script=debug,lightyear_prediction=debug";

fn main() -> ExitCode {
    let _log = Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    // Caught first, so a signal during the start stops the server at its first frame.
    let stop = match StopSignals::catch() {
        Ok(stop) => stop,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the stop's signals are not caught");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let args = match Logging::command_line(env::args_os()) {
        Ok(line) => Args::of(line),
        Err(status) => return ExitCode::from(status),
    };
    let bots = match args.server_bots() {
        Ok(bots) => bots,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "a bot's orders do not read");
            return ExitCode::from(ExitStatus::Usage);
        }
    };
    let Args {
        data,
        times,
        mode,
        map,
        host,
        address,
        port_map,
        ..
    } = args;
    let data = match ServerDir::open(&data) {
        Ok(data) => data,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the data directory does not open");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let key = match KeyFile::read_or_create(&data.layout().key_file(), Os::SYSTEM.entropy) {
        Ok(key) => key,
        Err(error) => {
            error!(key = %data.layout().key_file(), error = %ErrorReport::of(&error), "the server's key does not open");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    // The socket binds before any session opens, so a port in use ends the start and leaves the
    // data as it was.
    let secret = Zeroizing::new(key.secret_bytes());
    let endpoint = match QuicEndpoint::server(address, &secret, LinkTimes::CAMPFIRE) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            error!(%address, error = %ErrorReport::of(&error), "the server's socket does not open");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    drop(secret);
    let tls_key = endpoint.tls_key();
    // A waiting session names its own map, which its restore, or its end, replays.
    let waiting = Opening::waiting(&data).and_then(|waiting| {
        let journal = waiting.as_ref().map(|session| &session.private.terms.map);
        let map = Opening::map(journal, map)?;
        Ok((waiting, map))
    });
    let (waiting, map) = match waiting {
        Ok(found) => found,
        Err(error) => {
            error!(data = %data.layout().path().display(), error = %ErrorReport::of(&error), "no session starts");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let mode_dir = PackageDir::new(&mode);
    let packages = mode_dir
        .choose_map(map)
        .and_then(|map| mode_dir.mode_packages(&map));
    let packages = match packages {
        Ok(packages) => packages,
        Err(error) => {
            error!(mode = %mode.display(), error = %ErrorReport::of(&error), "the mode does not load");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let capabilities = packages.manifest().capabilities;
    let Started { opening, server } =
        match Started::open(&data, waiting, packages, key, times, bots, host) {
            Ok(started) => started,
            Err(code) => return code,
        };
    let local = endpoint.local_address();
    let limit = AddressLimit::new(opening.seats(), server.times.grace);
    let listening = announce(&opening, &mode, local, tls_key, &server.key);
    let mut app = server_app(opening, data, listening, capabilities, stop);
    let port_map = port_map.then(|| PortMap::start(local)).flatten();
    let server = app.world_mut().spawn((RawServer, endpoint, limit)).id();
    app.world_mut().trigger(Start { entity: server });
    let exit = app.run();
    // The endpoint closes each link before the mapping goes, whose delete can wait a UPnP
    // request's 10 s, which no client waits for (design 15, D12).
    drop(app);
    drop(port_map);
    ProcessExit::code(exit)
}

/// What the server starts with: its session and its setup.
#[derive(Debug)]
struct Started {
    opening: Opening,
    server: ServerSetup,
}

impl Started {
    /// The session `waiting` in the data directory `data` to restore, or a new one of the mode
    /// `packages` holds, with `bots` and the visibility `host` sets; and the server's setup of
    /// `key` and `times`. The exit code when one fails.
    fn open(
        data: &ServerDir,
        waiting: Option<RestoredSession>,
        packages: ModePackages,
        key: Keypair,
        times: SessionTimes,
        bots: ServerBots,
        host: HostVisibility,
    ) -> Result<Started, ExitCode> {
        let no_session = |error: OpeningError| {
            error!(data = %data.layout().path().display(), error = %ErrorReport::of(&error), "no session starts");
            ExitCode::from(ExitStatus::Failure)
        };
        let found = Opening::find(
            waiting,
            data,
            &packages,
            times.restore_window,
            key,
            Os::SYSTEM.entropy,
        )
        .map_err(no_session)?;
        let server = ServerSetup {
            key,
            times,
            os: Os::SYSTEM,
        };
        let setup = OpeningSetup {
            data,
            packages,
            server: server.clone(),
            bots,
            host,
            segments: SEGMENTS,
        };
        let opening = Opening::of(found, setup).map_err(|error| match error {
            OpeningError::KeepFiles(KeepFilesError::Dir(_) | KeepFilesError::Journal(_)) => {
                JournalFailed {
                    error: ErrorReport::of(&error).to_string(),
                }
                .log();
                ExitCode::from(ExitStatus::Storage)
            }
            error => no_session(error),
        })?;
        Ok(Started { opening, server })
    }
}

/// The server's app: its plugins at the rate of the session of `opening`, that session, a
/// `listening` event logged once its transport listens, and the end once every player left, or at
/// a signal `stop` caught, with its code, the session left in its journal for the next start to
/// restore (design 10, Stop).
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
        },
        ScheduleRunnerPlugin::run_loop(NetProtocol::FRAME),
    ));
    match opening.session {
        Opened::New(lobby) => app.insert_resource(*lobby),
        Opened::Restored(restore) => app.insert_resource(*restore),
    };
    app.insert_resource(data);
    app.add_observer(
        move |added: On<'_, '_, Add, Linked>, servers: Query<'_, '_, (), With<RawServer>>| {
            if servers.contains(added.entity) {
                listening.log();
            }
        },
    );
    app.add_systems(
        Update,
        (exit_when_due, move |world: &mut World| {
            if stop.received() {
                world.write_message(AppExit::from_code(StopSignals::EXIT_CODE));
            }
        })
            .chain()
            .after(PendingRestore::run),
    );
    app
}

/// What the server logs once its transport listens for the session `opening` starts: how a
/// player joins it, through its socket's address `address` and the TLS key `tls_key`; and logs
/// now, for a new session, that it opened, of the mode `mode`.
fn announce(
    opening: &Opening,
    mode: &Path,
    address: SocketAddr,
    tls_key: TlsKey,
    key: &Keypair,
) -> Listening {
    let terms = opening.terms();
    if let Opened::New(_) = opening.session {
        info!(
            session = %terms.session_id(),
            %address,
            players = terms.slots.len(),
            mode = %mode.display(),
            "opened a session"
        );
    }
    let key = key.x_only_public_key().0;
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
    if let Some(exit) = world.exit_due(false) {
        world.write_message(exit);
    }
}
