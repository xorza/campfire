//! Headless game server: opens a session of a mode on an address, lets its players join over
//! WebTransport, runs the match, and writes the session log once every player left. Its data
//! directory, which it holds locked while it runs, keeps its key.
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
use std::time::Duration;

use bevy_app::{App, ScheduleRunnerPlugin, TaskPoolPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_capabilities::CapabilitySet;
use campfire_common::ExitStatus;
use campfire_log::{ErrorReport, LogEvent, Logging};
use campfire_net::{
    JournalFailed, KeyFile, Listening, NetProtocol, Os, ProcessExit, ServerBots, ServerDir,
    ServerExit, ServerSetup, SessionTimes, SimServer,
};
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::Keypair;
use campfire_sim::TickRate;
use lightyear::prelude::server::{RawServer, Start, WebTransportServerIo};
use lightyear::prelude::{Linked, LocalAddr};
use tracing::{error, info};

use crate::args::Args;
use crate::opening::error::OpeningError;
use crate::opening::{Opening, OpeningSetup, Restore};
use crate::server_tls::ServerTls;

mod args;
mod opening;
mod server_tls;

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
        address,
        ..
    } = args;
    let data = match ServerDir::open(&data) {
        Ok(data) => data,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the data directory does not open");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let key = match KeyFile::read_or_create(&data.layout().key_file(), Os::fill) {
        Ok(key) => key,
        Err(error) => {
            error!(key = %data.layout().key_file(), error = %ErrorReport::of(&error), "the server's key does not open");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let mode_dir = PackageDir::new(&mode);
    let packages = ModePackages::choose_map(&mode_dir, map)
        .and_then(|map| ModePackages::from_package_dir(&mode_dir, &map));
    let packages = match packages {
        Ok(packages) => packages,
        Err(error) => {
            error!(mode = %mode.display(), error = %ErrorReport::of(&error), "the mode does not load");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let capabilities = packages.manifest().capabilities;
    let Started {
        opening,
        server,
        tls,
    } = match Started::open(&data, packages, key, times, bots) {
        Ok(started) => started,
        Err(code) => return code,
    };
    let certificate = tls.certificate();
    let listening = announce(&opening, &mode, address, certificate, &server.key);
    let tick = TickRate::new(opening.terms().tick_hz).length();
    let mut app = server_app(opening, data, tick, listening, capabilities);
    let server = app
        .world_mut()
        .spawn((
            RawServer,
            WebTransportServerIo {
                certificate: tls.into_identity(),
            },
            LocalAddr(address),
        ))
        .id();
    app.world_mut().trigger(Start { entity: server });
    ProcessExit::code(app.run())
}

/// What the server starts with: its session, its setup, and its TLS identity.
#[derive(Debug)]
struct Started {
    opening: Opening,
    server: ServerSetup,
    tls: ServerTls,
}

impl Started {
    /// The session the data directory `data` holds to restore, or a new one of the mode
    /// `packages` holds, with `bots`; the TLS identity, made again only when no session
    /// restores; and the server's setup of `key` and `times`. The exit code when one fails.
    fn open(
        data: &ServerDir,
        packages: ModePackages,
        key: Keypair,
        times: SessionTimes,
        bots: ServerBots,
    ) -> Result<Started, ExitCode> {
        let no_session = |error: OpeningError| {
            error!(data = %data.layout().path().display(), error = %ErrorReport::of(&error), "no session starts");
            ExitCode::from(ExitStatus::Failure)
        };
        let found = Opening::find(data, &packages, times.restore_window, key, Os::fill)
            .map_err(no_session)?;
        let tls = ServerTls::open(&data.layout().tls_file(), Os::unix_now(), found.is_some()).map_err(
            |error| {
                error!(data = %data.layout().path().display(), error = %ErrorReport::of(&error), "the TLS identity does not open");
                ExitCode::from(ExitStatus::Failure)
            },
        )?;
        let server = ServerSetup {
            key,
            certificate: tls.certificate(),
            times,
            clock: Os::unix_now,
            entropy: Os::fill,
        };
        let setup = OpeningSetup {
            data,
            packages,
            server,
            bots,
            segments: SEGMENTS,
        };
        let opening = Opening::of(found, setup).map_err(|error| match error {
            OpeningError::NewSession(_) | OpeningError::NewJournal(_) => {
                JournalFailed {
                    error: ErrorReport::of(&error).to_string(),
                }
                .log();
                ExitCode::from(ExitStatus::Storage)
            }
            error => no_session(error),
        })?;
        Ok(Started {
            opening,
            server,
            tls,
        })
    }
}

/// The server's app: its plugins at `tick` a tick, the session of `opening`, a `listening`
/// event logged once its transport listens, and the end once every player left.
fn server_app(
    opening: Opening,
    data: ServerDir,
    tick: Duration,
    listening: Listening,
    capabilities: CapabilitySet,
) -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        TimePlugin,
        StatesPlugin,
        ScheduleRunnerPlugin::run_loop(NetProtocol::FRAME),
    ));
    app.add_plugins(SimServer { tick, capabilities });
    match opening {
        Opening::New(lobby) => app.insert_resource(*lobby),
        Opening::Restored(restore) => app.insert_resource(*restore),
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
        (
            Restore::run.run_if(resource_exists::<Restore>),
            exit_when_due,
        )
            .chain(),
    );
    app
}

/// What the server logs once its transport listens for the session `opening` starts: how a
/// player joins it, through `address` and the certificate of `certificate`'s hash; and logs now,
/// for a new session, that it opened, of the mode `mode`.
fn announce(
    opening: &Opening,
    mode: &Path,
    address: SocketAddr,
    certificate: CertificateHash,
    key: &Keypair,
) -> Listening {
    let terms = opening.terms();
    if let Opening::New(_) = opening {
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
        certificate,
        server_key: key,
        tick_hz,
        join: format!(
            "campfire-client {} <this machine's LAN address>:{} {certificate} {key} {tick_hz}",
            mode.display(),
            address.port()
        ),
    }
}

/// Exits as `ServerExit::due` says: the server stops only once its session ended.
fn exit_when_due(world: &mut World) {
    if let Some(exit) = ServerExit::due(world, false) {
        world.write_message(exit);
    }
}
