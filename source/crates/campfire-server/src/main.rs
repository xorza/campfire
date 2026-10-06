//! Headless game server: opens a session of a mode on an address, lets its players join over
//! WebTransport, runs the match, and writes the session log once every player left. Its data
//! directory, which it holds locked while it runs, keeps its key.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info` by default). With `CAMPFIRE_LOG` set
//! to a path, they also go there as JSON lines, filtered by `CAMPFIRE_LOG_FILTER` (Campfire's
//! `debug` by default).

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, TaskPoolPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::{Mut, World};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_log::{LogEvent, Logging};
use campfire_net::{
    DataDir, JournalFailed, Listening, NetProtocol, OrderScript, ServerBots, ServerExit,
    ServerSetup, SessionTimes, SimServer, SlotBot,
};
use campfire_package::ModePackages;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::Keypair;
use campfire_sim::TickRate;
use lightyear::prelude::server::{RawServer, ServerPlugins, Start, WebTransportServerIo};
use lightyear::prelude::{LinkOf, Linked, LocalAddr, ReplicationSender};
use tracing::{error, info};

use crate::data_path::DataPath;
use crate::error::OpeningError;
use crate::opening::{Opening, OpeningSetup, Restore};
use crate::server_config::ServerConfig;
use crate::server_tls::ServerTls;

mod data_path;
mod error;
mod opening;
mod server_config;
mod server_tls;

/// The segments of a session's seed chain: the most checkpoints it may take, less one.
const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

const USAGE: &str = "usage: campfire-server --data <data directory> [--restore-window <seconds>] \
                     [--grace <seconds>] [--bot <slot>=<orders file>]... [--takeover <orders \
                     file>] <mode package directory> <address, as 0.0.0.0:4433>";

/// How often the app loop runs: often enough that no fixed tick waits long for its frame.
const FRAME: Duration = Duration::from_millis(2);

/// What the terminal shows when `RUST_LOG` does not say.
const TERMINAL_FILTER: &str = "info";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and Lightyear's rollbacks; of the rest, `info` and above.
const FILE_FILTER: &str = "info,campfire_server=debug,campfire_net=debug,campfire_runner=debug,\
                           campfire_script=debug,lightyear_prediction=debug";

fn main() -> ExitCode {
    Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    let args = match Args::parse(env::args_os().skip(1)) {
        Ok(args) => args,
        Err(problem) => {
            error!(%problem, USAGE);
            return ExitCode::from(2);
        }
    };
    let bots = match args.server_bots() {
        Ok(bots) => bots,
        Err(problem) => {
            error!(%problem, "a bot's orders do not read");
            return ExitCode::from(2);
        }
    };
    let Args {
        data,
        times,
        mode,
        address,
        ..
    } = args;
    let data_dir = match DataDir::open(&data, fill) {
        Ok(data_dir) => data_dir,
        Err(error) => {
            error!(data = %data.display(), %error, "the data directory does not open");
            return ExitCode::FAILURE;
        }
    };
    let packages = match ModePackages::from_dir(&mode) {
        Ok(packages) => packages,
        Err(error) => {
            error!(mode = %mode.display(), %error, "the mode does not load");
            return ExitCode::FAILURE;
        }
    };
    let Started {
        opening,
        server,
        tls,
    } = match Started::open(&data, packages, data_dir.key, times, bots) {
        Ok(started) => started,
        Err(code) => return code,
    };
    let certificate = tls.certificate();
    let listening = announce(&opening, &mode, address, certificate, &server.key);
    let tick = TickRate::new(opening.terms().tick_hz).length();
    let mut app = server_app(
        opening,
        ServerConfig(server),
        DataPath(data),
        tick,
        listening,
    );
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
    exit_code(app.run())
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
        data: &Path,
        packages: ModePackages,
        key: Keypair,
        times: SessionTimes,
        bots: ServerBots,
    ) -> Result<Started, ExitCode> {
        let no_session = |error: OpeningError| {
            error!(data = %data.display(), %error, "no session starts");
            ExitCode::FAILURE
        };
        let found =
            Opening::find(data, &packages, times.restore_window, key, fill).map_err(no_session)?;
        let tls = ServerTls::open(data, unix_now(), found.is_some()).map_err(|error| {
            error!(data = %data.display(), %error, "the TLS identity does not open");
            ExitCode::FAILURE
        })?;
        let server = ServerSetup {
            key,
            certificate: tls.certificate(),
            times,
            clock: unix_now,
            entropy: fill,
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
                    error: error.to_string(),
                }
                .log();
                ExitCode::from(ServerExit::JOURNAL_FAILED)
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
    config: ServerConfig,
    data: DataPath,
    tick: Duration,
    listening: Listening,
) -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        TimePlugin,
        StatesPlugin,
        ScheduleRunnerPlugin::run_loop(FRAME),
    ));
    app.add_plugins(ServerPlugins {
        tick_duration: tick,
    });
    app.add_plugins((NetProtocol, SimServer));
    match opening {
        Opening::New(lobby) => app.insert_resource(*lobby),
        Opening::Restored(restore) => app.insert_resource(*restore),
    };
    app.insert_resource(config);
    app.insert_resource(data);
    app.add_observer(
        |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
            commands.entity(added.entity).insert(ReplicationSender);
        },
    );
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

/// What the command line names: the data directory, how long after its journal's last write a
/// session a stop ended restores, the mode to open, and the address to listen on.
#[derive(Debug)]
struct Args {
    data: PathBuf,
    times: SessionTimes,
    bots: Vec<BotFile>,
    takeover: Option<PathBuf>,
    mode: PathBuf,
    address: SocketAddr,
}

/// The index of a slot the server's bot plays, and the order file of its script.
#[derive(Debug)]
struct BotFile {
    slot: u32,
    path: PathBuf,
}

impl Args {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Args, String> {
        let mut args = args.peekable();
        let (Some(flag), Some(data)) = (args.next(), args.next()) else {
            return Err("--data and its directory are needed".to_owned());
        };
        if flag != "--data" {
            return Err(format!("{}: not --data", flag.display()));
        }
        let mut parsed = Args {
            data: PathBuf::from(data),
            times: SessionTimes::DEFAULT,
            bots: Vec::new(),
            takeover: None,
            mode: PathBuf::new(),
            address: SocketAddr::from(([0, 0, 0, 0], 0)),
        };
        while let Some(flag) =
            args.next_if(|arg| arg.to_str().is_some_and(|arg| arg.starts_with("--")))
        {
            let value = args
                .next()
                .ok_or_else(|| format!("{}: no value", flag.display()))?;
            match flag.to_str() {
                Some("--grace") => parsed.times.grace = Args::seconds(&value)?,
                Some("--restore-window") => parsed.times.restore_window = Args::seconds(&value)?,
                Some("--bot") => parsed.bots.push(Args::bot(&value)?),
                Some("--takeover") => parsed.takeover = Some(PathBuf::from(value)),
                _ => return Err(format!("{}: no such flag", flag.display())),
            }
        }
        let (Some(mode), Some(address), None) = (args.next(), args.next(), args.next()) else {
            return Err("the mode and the address are needed, and nothing after".to_owned());
        };
        parsed.mode = PathBuf::from(mode);
        parsed.address = address
            .to_str()
            .and_then(|text| text.parse::<SocketAddr>().ok())
            .ok_or_else(|| format!("{}: not a socket address", address.display()))?;
        Ok(parsed)
    }

    fn seconds(value: &OsString) -> Result<Duration, String> {
        value
            .to_str()
            .and_then(|text| text.parse().ok())
            .map(Duration::from_secs)
            .ok_or_else(|| format!("{}: not a whole number of seconds", value.display()))
    }

    /// A bot's slot and file, as `<slot>=<file>`.
    fn bot(value: &OsString) -> Result<BotFile, String> {
        let text = value
            .to_str()
            .ok_or_else(|| format!("{}: not text", value.display()))?;
        let (slot, path) = text
            .split_once('=')
            .ok_or_else(|| format!("{text}: not <slot>=<orders file>"))?;
        let slot = slot
            .parse()
            .map_err(|error| format!("{slot}: not a slot number: {error}"))?;
        Ok(BotFile {
            slot,
            path: PathBuf::from(path),
        })
    }

    /// The server's bots, their scripts read from their files; an error naming a file that does
    /// not read.
    fn server_bots(&self) -> Result<ServerBots, String> {
        let read = |path: &Path| {
            let text =
                fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
            OrderScript::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
        };
        let mut slots = Vec::with_capacity(self.bots.len());
        for bot in &self.bots {
            slots.push(SlotBot::new(bot.slot, read(&bot.path)?));
        }
        let takeover = self.takeover.as_deref().map(read).transpose()?;
        Ok(ServerBots { slots, takeover })
    }
}

/// The process's exit code for how the app exited.
fn exit_code(exit: AppExit) -> ExitCode {
    match exit {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

/// Exits as `ServerExit::due` says: the server stops only once its session ended.
fn exit_when_due(world: &mut World) {
    let exit = world
        .resource_scope(|world, data: Mut<'_, DataPath>| ServerExit::due(world, &data.0, false));
    if let Some(exit) = exit {
        world.write_message(exit);
    }
}

fn fill(bytes: &mut [u8; 32]) {
    getrandom::fill(bytes).expect("the OS gives random bytes");
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_reads_its_flags_in_any_order_or_its_flaw() {
        let parse = |args: &[&str]| Args::parse(args.iter().map(OsString::from));
        let args = parse(&[
            "--data",
            "d",
            "--bot",
            "1=bot.toml",
            "--grace",
            "5",
            "--takeover",
            "takeover.toml",
            "--restore-window",
            "9",
            "mode",
            "0.0.0.0:4433",
        ])
        .unwrap();
        assert_eq!(args.data, PathBuf::from("d"));
        assert_eq!(
            (args.times.grace, args.times.restore_window),
            (Duration::from_secs(5), Duration::from_secs(9))
        );
        assert_eq!(
            args.bots
                .iter()
                .map(|bot| (bot.slot, bot.path.clone()))
                .collect::<Vec<_>>(),
            [(1, PathBuf::from("bot.toml"))]
        );
        assert_eq!(args.takeover, Some(PathBuf::from("takeover.toml")));
        assert_eq!(args.mode, PathBuf::from("mode"));
        // The defaults, with no flag.
        let plain = parse(&["--data", "d", "mode", "0.0.0.0:4433"]).unwrap();
        assert_eq!(plain.times, SessionTimes::DEFAULT);
        assert!(plain.bots.is_empty() && plain.takeover.is_none());
        for flawed in [
            &["--data", "d", "--grace", "soon", "mode", "0.0.0.0:4433"][..],
            &["--data", "d", "--bot", "1", "mode", "0.0.0.0:4433"],
            &["--data", "d", "--bot", "x=bot.toml", "mode", "0.0.0.0:4433"],
            &["--data", "d", "--fast", "1", "mode", "0.0.0.0:4433"],
            &["--data", "d", "mode"],
        ] {
            assert!(parse(flawed).is_err(), "{flawed:?}");
        }
    }
}
