//! Game client: joins a session over WebTransport, predicts the player's own hero, and draws the
//! match as capsules on the ground; a right click walks the hero there. With `--bot <orders
//! file>`, it opens no window and renders nothing, and plays the file's `OrderScript` instead.
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
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::DefaultPlugins;
use bevy::app::{App, AppExit, PluginGroup, ScheduleRunnerPlugin, TaskPoolPlugin};
use bevy::log::LogPlugin;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use bevy::window::{Window, WindowPlugin};
use campfire_log::Logging;
use campfire_net::{ClientMode, NetProtocol, OrderScript, ServerPin, SimClient};
use campfire_package::ModePackages;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_sim::TickRate;
use lightyear::prelude::client::{ClientPlugins, RawClient, WebTransportClientIo};
use lightyear::prelude::{
    Client, Connect, LocalAddr, PeerAddr, PredictionManager, ReplicationReceiver,
};
use tracing::error;

use crate::bot::Bot;
use crate::orders::Orders;
use crate::view::View;

mod bot;
mod orders;
mod view;

/// What the command line names: the orders file a bot plays, the mode to play, and the server as
/// its listing gives it.
#[derive(Debug)]
struct Args {
    bot: Option<PathBuf>,
    mode: PathBuf,
    address: SocketAddr,
    certificate: CertificateHash,
    server_key: XOnlyPublicKey,
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
                "usage: campfire-client [--bot <orders file>] <mode package directory> \
                 <server address> <certificate hash> <server key>"
            );
            return ExitCode::from(2);
        }
    };
    let packages = match ModePackages::from_dir(&args.mode) {
        Ok(packages) => packages,
        Err(error) => {
            error!(mode = %args.mode.display(), %error, "the mode does not load");
            return ExitCode::FAILURE;
        }
    };
    let script = match args.bot.as_deref().map(read_script).transpose() {
        Ok(script) => script,
        Err(problem) => {
            error!(%problem, "the orders file does not read");
            return ExitCode::FAILURE;
        }
    };
    let mode = ClientMode::of(&packages);
    let tick = TickRate::new(mode.tick_hz).length();

    let mut app = App::new();
    if let Some(script) = script {
        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            StatesPlugin,
            ScheduleRunnerPlugin::run_loop(BOT_FRAME),
            Bot { script },
        ));
    } else {
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
            Orders,
        ));
    }
    app.add_plugins(ClientPlugins {
        tick_duration: tick,
    });
    app.add_plugins((
        NetProtocol,
        SimClient {
            main_key: keypair(),
            session_key: keypair(),
            server: ServerPin {
                key: args.server_key.serialize(),
                certificate: args.certificate,
            },
            mode,
            clock: unix_now,
            entropy: fill,
        },
    ));
    app.insert_resource(PredictionManager::default());
    let client = app
        .world_mut()
        .spawn((
            Client,
            RawClient,
            ReplicationReceiver,
            WebTransportClientIo {
                certificate_digest: args.certificate.to_string(),
                target: None,
            },
            PeerAddr(args.address),
            // A raw client counts as connected once linked only with a local address, which only
            // names it; the transport binds its own socket.
            LocalAddr(SocketAddr::from(([0, 0, 0, 0], 0))),
        ))
        .id();
    app.world_mut().trigger(Connect { entity: client });
    match app.run() {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

impl Args {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Args, String> {
        let mut args = args.peekable();
        let bot = if args.next_if(|arg| arg == "--bot").is_some() {
            Some(PathBuf::from(
                args.next().ok_or("--bot needs an orders file")?,
            ))
        } else {
            None
        };
        let (Some(mode), Some(address), Some(certificate), Some(server_key), None) = (
            args.next(),
            args.next(),
            args.next(),
            args.next(),
            args.next(),
        ) else {
            return Err("four arguments are needed after the options".to_owned());
        };
        let text = |arg: &OsString| {
            arg.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{}: not UTF-8", arg.display()))
        };
        let address = text(&address)?;
        let certificate = text(&certificate)?;
        let server_key = text(&server_key)?;
        Ok(Args {
            bot,
            mode: PathBuf::from(mode),
            address: address
                .parse()
                .map_err(|error| format!("{address}: {error}"))?,
            certificate: certificate
                .parse()
                .map_err(|error| format!("{certificate}: {error}"))?,
            server_key: XOnlyPublicKey::from_str(&server_key)
                .map_err(|error| format!("{server_key}: {error}"))?,
        })
    }
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
