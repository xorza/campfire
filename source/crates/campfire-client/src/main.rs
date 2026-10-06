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
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bevy::DefaultPlugins;
use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin, TaskPoolPlugin};
use bevy::log::LogPlugin;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use bevy::window::{Window, WindowPlugin};
use campfire_common::ExitStatus;
use campfire_log::{ErrorReport, Logging};
use campfire_net::{
    ClientDir, KeyFile, NetProtocol, OrderScript, Os, Pace, ProcessExit, SimClient,
};
use campfire_package::ModePackages;
use campfire_protocol::RandomKey;
use campfire_protocol::secp256k1::Keypair;
use campfire_runner::SessionRules;
use campfire_sim::TickRate;
use lightyear::prelude::Connect;
use tracing::error;

use crate::args::error::ArgsError;
use crate::args::{Args, Server};
use crate::bot::Bot;
use crate::connection::Connection;
use crate::hud::Hud;
use crate::link_watch::LinkWatch;
use crate::local_keys::LocalKeys;
use crate::orders::Orders;
use crate::view::View;

mod args;
mod bot;
mod connection;
mod hud;
mod link_watch;
mod local_keys;
mod orders;
mod pointer;
mod view;

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
    let args = match Args::read(env::args_os()) {
        Ok(args) => args,
        Err(ArgsError::CommandLine(output)) if !output.use_stderr() => return shown(&output),
        Err(ArgsError::CommandLine(error)) => {
            error!(error = %error, "the command line is refused");
            return ExitCode::from(ExitStatus::Usage);
        }
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the command line is refused");
            return ExitCode::from(ExitStatus::Usage);
        }
    };
    let packages = match load_mode(&args) {
        Ok(packages) => Arc::new(packages),
        Err(code) => return code,
    };
    let script = match args.bot.as_deref().map(OrderScript::read).transpose() {
        Ok(script) => script,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the orders file does not read");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let main_key = match main_key(&args) {
        Ok(key) => key,
        Err(code) => return code,
    };
    let data = match &args.data {
        None => None,
        Some(path) => match ClientDir::open(path) {
            Ok(data) => Some(Arc::new(data)),
            Err(error) => {
                error!(data = %path.display(), error = %ErrorReport::of(&error), "the data directory does not open");
                return ExitCode::from(ExitStatus::Failure);
            }
        },
    };
    let pace = Arc::new(Pace::default());
    let mut connection = match Connection::open(&args, data.as_deref(), &packages, &pace) {
        Ok(connection) => connection,
        Err(code) => return code,
    };
    let pin = connection.pin();
    let tick = TickRate::new(pin.tick_hz).length();

    let mut app = App::new();
    let local = matches!(connection, Connection::Local(_));
    add_ends(&mut app, script, local.then_some(&pace), tick);
    app.add_plugins((
        SimClient {
            main_key,
            session_key: RandomKey::generate(Os::fill),
            server: pin,
            local,
            packages,
            clock: Os::unix_now,
            entropy: Os::fill,
            data,
        },
        LinkWatch,
    ));
    let client = SimClient::spawn_client(app.world_mut());
    connection.link(&mut app, client, &pace, tick);
    app.world_mut().trigger(Connect { entity: client });
    let exit = app.run();
    // Dropped, a local server ends the session and publishes its log.
    drop(connection);
    ProcessExit::code(exit)
}

/// Adds the client's own end: a bot playing `script`, with no window, or the view, the HUD and the
/// orders, with the keys of a local match that `pace` follows.
fn add_ends(app: &mut App, script: Option<OrderScript>, pace: Option<&Arc<Pace>>, tick: Duration) {
    if let Some(script) = script {
        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            StatesPlugin,
            ScheduleRunnerPlugin::run_loop(NetProtocol::FRAME),
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
        error!(mode = %args.mode.display(), error = %ErrorReport::of(&error), "the mode does not load");
        ExitCode::from(ExitStatus::Failure)
    })?;
    if let Server::Remote { tick_hz, .. } = args.server {
        SessionRules::of(&packages)
            .runs_at(tick_hz)
            .map_err(|error| {
                error!(error = %ErrorReport::of(&error), "the mode does not run at the listing's rate");
                ExitCode::from(ExitStatus::Usage)
            })?;
    }
    Ok(packages)
}

/// Ends the client once clap printed the help or the version `output` asked for, to standard
/// output: with success, or with failure when it does not print.
fn shown(output: &clap::Error) -> ExitCode {
    match output.print() {
        Ok(()) => ExitCode::from(ExitStatus::Success),
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the help does not print");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}

/// The player's main key: the key file's, made when missing, or a new one with no file; the exit
/// code when the file does not read.
fn main_key(args: &Args) -> Result<Keypair, ExitCode> {
    let Some(path) = &args.key else {
        return Ok(RandomKey::generate(Os::fill));
    };
    KeyFile::read_or_create(path, Os::fill).map_err(|error| {
        error!(key = %path.display(), error = %ErrorReport::of(&error), "the key file does not read");
        ExitCode::from(ExitStatus::Failure)
    })
}
