//! Game client: joins a session over WebTransport, predicts the player's own avatar, and draws the
//! match as capsules on the ground; a right click walks the avatar there. With `--bot <orders
//! file>`, it opens no window and renders nothing, and plays the file's `OrderScript` instead.
//! With `--key <file>`, the player's main key is the file's, made when missing; without it, the
//! player is a new key each run. With `--data <directory>`, the client writes the newest receipt
//! of its session there; without it, it writes none. With `--local`, it plays the mode alone on a
//! local server, a thread of its own process, whose data goes in `server` under the data
//! directory, `--data` then needed; each `--server-bot <slot>=<orders file>` gives the server a bot
//! there, and every other slot but the player's is open. P pauses a local match, the keys 1 to 4
//! set its speed to 0.5, 1, 2 or 4 times, F5 saves it, and F9 loads its latest save.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info`, and the renderer's warnings, by
//! default). With `CAMPFIRE_LOG` set to a path, they also go there as JSON lines, filtered by
//! `CAMPFIRE_LOG_FILTER` (Campfire's `debug` by default).

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
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::DefaultPlugins;
use bevy::app::{App, AppExit, PluginGroup, ScheduleRunnerPlugin, TaskPoolPlugin};
use bevy::log::LogPlugin;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use bevy::window::{Window, WindowPlugin};
use campfire_log::Logging;
use campfire_net::{NetProtocol, OrderScript, Pace, SimClient};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_protocol::{CertificateHash, KeyFile};
use campfire_runner::SessionRules;
use campfire_sim::TickRate;
use lightyear::prelude::client::{ClientPlugins, RawClient};
use lightyear::prelude::{Client, Connect, PredictionManager, ReplicationReceiver};
use tracing::error;

use crate::bot::Bot;
use crate::connection::Connection;
use crate::hud::Hud;
use crate::link_watch::LinkWatch;
use crate::local_keys::LocalKeys;
use crate::orders::Orders;
use crate::view::View;

mod bot;
mod connection;
mod hud;
mod link_watch;
mod local_keys;
mod orders;
mod pointer;
mod view;

/// What the command line names: the orders file a bot plays, the player's key file, the data
/// directory, the mode to play, and the server: as its listing gives it, or a local one.
#[derive(Debug)]
struct Args {
    bot: Option<PathBuf>,
    key: Option<PathBuf>,
    data: Option<PathBuf>,
    mode: PathBuf,
    server: Server,
}

/// The server the client plays on.
#[derive(Debug)]
enum Server {
    /// A server elsewhere, as its listing gives it.
    Remote {
        address: SocketAddr,
        certificate: CertificateHash,
        key: XOnlyPublicKey,
        tick_hz: NonZeroU32,
    },
    /// A local server, on a thread of the client's process, with the bots of `bots`.
    Local { bots: Vec<BotFile> },
}

/// A server bot of a local server: its slot, and the file of the orders it plays.
#[derive(Debug)]
struct BotFile {
    slot: u32,
    path: PathBuf,
}

/// How often a bot's app loop runs: often enough that no fixed tick waits long for its frame.
const BOT_FRAME: Duration = Duration::from_millis(2);
/// What the terminal shows when `RUST_LOG` does not say: the renderer's validation layers report
/// through `wgpu_hal`, loudly, in debug builds.
const TERMINAL_FILTER: &str = "info,wgpu=error,wgpu_hal=off,naga=warn";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and Lightyear's rollbacks; of the rest, `info` and above, without the renderer's
/// validation layers.
const FILE_FILTER: &str = "info,campfire_client=debug,campfire_net=debug,campfire_script=debug,\
                           lightyear_prediction=debug,wgpu=warn,wgpu_hal=off,naga=warn";

fn main() -> ExitCode {
    Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    let args = match Args::parse(env::args_os().skip(1)) {
        Ok(args) => args,
        Err(problem) => {
            error!(
                %problem,
                "usage: campfire-client [--bot <orders file>] [--key <key file>] [--data <data \
                 directory>] <mode package directory> <server address> <certificate hash> \
                 <server key> <tick rate>; or campfire-client --local --data <data directory> \
                 [--server-bot <slot>=<orders file>]... [--bot <orders file>] [--key <key \
                 file>] <mode package directory>"
            );
            return ExitCode::from(2);
        }
    };
    let packages = match load_mode(&args) {
        Ok(packages) => packages,
        Err(code) => return code,
    };
    let script = match args.bot.as_deref().map(read_script).transpose() {
        Ok(script) => script,
        Err(problem) => {
            error!(%problem, "the orders file does not read");
            return ExitCode::FAILURE;
        }
    };
    let main_key = match main_key(&args) {
        Ok(key) => key,
        Err(code) => return code,
    };
    let pace = Arc::new(Pace::default());
    let mut connection = match Connection::open(&args, &pace) {
        Ok(connection) => connection,
        Err(code) => return code,
    };
    let pin = connection.pin();
    let tick = TickRate::new(pin.tick_hz).length();

    let mut app = App::new();
    let local = matches!(connection, Connection::Local(_));
    add_ends(&mut app, script, local.then_some(&pace), tick);
    app.add_plugins((
        ClientPlugins {
            tick_duration: tick,
        },
        LinkWatch,
    ));
    app.add_plugins((
        NetProtocol,
        SimClient {
            main_key,
            session_key: keypair(),
            server: pin,
            local,
            packages: Arc::new(packages),
            clock: unix_now,
            entropy: fill,
            data: args.data.clone(),
        },
    ));
    app.insert_resource(PredictionManager::default());
    let client = app
        .world_mut()
        .spawn((Client, RawClient, ReplicationReceiver))
        .id();
    connection.link(&mut app, client, &pace, tick);
    app.world_mut().trigger(Connect { entity: client });
    let exit = app.run();
    // Dropped, a local server ends the session and publishes its log.
    drop(connection);
    match exit {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

impl Args {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Args, String> {
        let mut args = args.peekable();
        let (mut bot, mut key, mut data) = (None, None, None);
        let mut local = false;
        let mut bots = Vec::new();
        while let Some(flag) =
            args.next_if(|arg| arg.to_str().is_some_and(|arg| arg.starts_with("--")))
        {
            if flag == "--local" {
                local = true;
                continue;
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{} needs a value", flag.display()))?;
            if flag == "--server-bot" {
                bots.push(BotFile::parse(&value)?);
                continue;
            }
            let slot = if flag == "--bot" {
                &mut bot
            } else if flag == "--key" {
                &mut key
            } else if flag == "--data" {
                &mut data
            } else {
                return Err(format!("{}: no such option", flag.display()));
            };
            if slot.replace(PathBuf::from(value)).is_some() {
                return Err(format!("{} given twice", flag.display()));
            }
        }
        let mode = PathBuf::from(args.next().ok_or("the mode is needed after the options")?);
        let server = if local {
            if data.is_none() {
                return Err("--local needs --data".to_owned());
            }
            if args.next().is_some() {
                return Err("--local takes the mode alone".to_owned());
            }
            Server::Local { bots }
        } else {
            if !bots.is_empty() {
                return Err("--server-bot needs --local".to_owned());
            }
            Server::parse(args)?
        };
        Ok(Args {
            bot,
            key,
            data,
            mode,
            server,
        })
    }
}

impl Server {
    /// The remote server the arguments after the mode name: its address, its certificate's
    /// hash, its key and its tick rate.
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Server, String> {
        let (Some(address), Some(certificate), Some(key), Some(tick_hz), None) = (
            args.next(),
            args.next(),
            args.next(),
            args.next(),
            args.next(),
        ) else {
            return Err("four arguments are needed after the mode".to_owned());
        };
        let text = |arg: &OsString| {
            arg.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{}: not UTF-8", arg.display()))
        };
        let address = text(&address)?;
        let certificate = text(&certificate)?;
        let key = text(&key)?;
        let tick_hz = text(&tick_hz)?;
        Ok(Server::Remote {
            address: address
                .parse()
                .map_err(|error| format!("{address}: {error}"))?,
            certificate: certificate
                .parse()
                .map_err(|error| format!("{certificate}: {error}"))?,
            key: XOnlyPublicKey::from_str(&key).map_err(|error| format!("{key}: {error}"))?,
            tick_hz: tick_hz
                .parse()
                .map_err(|error| format!("{tick_hz}: {error}"))?,
        })
    }
}

impl BotFile {
    /// The bot `<slot>=<orders file>` names; an error for text of another form, or slot 0, the
    /// client's.
    fn parse(value: &OsString) -> Result<BotFile, String> {
        let text = value
            .to_str()
            .ok_or_else(|| format!("{}: not text", value.display()))?;
        let (slot, path) = text
            .split_once('=')
            .ok_or_else(|| format!("{text}: not <slot>=<orders file>"))?;
        let slot: u32 = slot
            .parse()
            .map_err(|error| format!("{slot}: not a slot number: {error}"))?;
        if slot == 0 {
            return Err("slot 0 is the client's".to_owned());
        }
        Ok(BotFile {
            slot,
            path: PathBuf::from(path),
        })
    }
}

/// Adds the client's own end: a bot playing `script`, with no window, or the view, the HUD and the
/// orders, with the keys of a local match that `pace` follows.
fn add_ends(app: &mut App, script: Option<OrderScript>, pace: Option<&Arc<Pace>>, tick: Duration) {
    if let Some(script) = script {
        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            StatesPlugin,
            ScheduleRunnerPlugin::run_loop(BOT_FRAME),
            Bot { script },
        ));
        return;
    }
    if let Some(pace) = pace {
        app.add_plugins(LocalKeys {
            pace: Arc::clone(pace),
        });
    }
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Campfire".to_owned(),
                    ..Window::default()
                }),
                ..WindowPlugin::default()
            })
            .disable::<LogPlugin>(),
        View { tick },
        Hud,
        Orders,
    ));
}

/// The packages of the mode the arguments name, which must run at the listing's rate; the exit
/// code when they do not load, or run at another rate.
fn load_mode(args: &Args) -> Result<ModePackages, ExitCode> {
    let packages = ModePackages::from_dir(&args.mode).map_err(|error| {
        error!(mode = %args.mode.display(), %error, "the mode does not load");
        ExitCode::FAILURE
    })?;
    if let Server::Remote { tick_hz, .. } = args.server {
        SessionRules::of(&packages)
            .runs_at(tick_hz)
            .map_err(|error| {
                error!(%error, "the mode does not run at the listing's rate");
                ExitCode::from(2)
            })?;
    }
    Ok(packages)
}

/// The player's main key: the key file's, made when missing, or a new one with no file; the exit
/// code when the file does not read.
fn main_key(args: &Args) -> Result<Keypair, ExitCode> {
    let Some(path) = &args.key else {
        return Ok(keypair());
    };
    KeyFile::read_or_create(path, fill).map_err(|error| {
        error!(key = %path.display(), %error, "the key file does not read");
        ExitCode::FAILURE
    })
}

/// The order script in the file at `path`.
fn read_script(path: &Path) -> Result<OrderScript, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    OrderScript::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// A fresh key.
fn keypair() -> Keypair {
    loop {
        let mut secret = [0; 32];
        fill(&mut secret);
        if let Ok(secret) = SecretKey::from_byte_array(&secret) {
            return Keypair::from_secret_key(&Secp256k1::new(), &secret);
        }
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

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn the_command_line_names_a_remote_or_a_local_server() {
        let key = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let certificate = "03".repeat(32);
        let remote = parse(&[
            "--key",
            "k",
            "mode",
            "10.0.0.2:4433",
            &certificate,
            key,
            "30",
        ])
        .unwrap();
        assert_eq!((remote.key, remote.mode), (Some("k".into()), "mode".into()));
        assert!(matches!(
            remote.server,
            Server::Remote { address, tick_hz, .. }
                if address == SocketAddr::from(([10, 0, 0, 2], 4433)) && tick_hz.get() == 30
        ));

        let local = parse(&[
            "--local",
            "--data",
            "d",
            "--server-bot",
            "1=a.toml",
            "--server-bot",
            "2=b.toml",
            "mode",
        ])
        .unwrap();
        assert_eq!(local.data, Some("d".into()));
        let Server::Local { bots } = local.server else {
            panic!("a local server");
        };
        let bots: Vec<(u32, PathBuf)> = bots.into_iter().map(|bot| (bot.slot, bot.path)).collect();
        assert_eq!(bots, [(1, "a.toml".into()), (2, "b.toml".into())]);

        for (args, problem) in [
            (&["--local", "mode"][..], "--local needs --data"),
            (
                &["--local", "--data", "d", "mode", "x"],
                "--local takes the mode alone",
            ),
            (
                &["--server-bot", "1=a.toml", "mode"],
                "--server-bot needs --local",
            ),
            (
                &["--local", "--data", "d", "--server-bot", "0=a.toml", "mode"],
                "slot 0 is the client's",
            ),
            (&["--fast", "mode"], "--fast: no such option"),
        ] {
            assert_eq!(parse(args).err().as_deref(), Some(problem), "{args:?}");
        }
    }
}
