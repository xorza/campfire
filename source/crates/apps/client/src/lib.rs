//! The game client, which its binary runs: its command line, its connection, and what its window
//! draws and takes from the player.

use std::env;
use std::path;
use std::process::ExitCode;
use std::sync::Arc;

use bevy::app::App;
use campfire_bots::{BotNames, BotScript};
use campfire_common::ErrorReport;
use campfire_console::GameRole;
use campfire_host::{ConsoleStdio, KeyFile, KeyFileError, OrderFile, Os, SessionSigner, exit_code};
use campfire_logging::{LogTap, Logging};
use campfire_net::{ClientConsole, SimClientPlugin};
use campfire_net_connection::Connect;
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_protocol::Signer;
use campfire_store::SecretFile;
use tracing::error;

use crate::client_module::ClientEnd;
use crate::command_line::{Args, CommandLine, ConsoleArg};
use crate::connection::Connection;
use crate::error::ClientLaunchError;
use crate::link_watch::LinkWatchPlugin;

mod action_names;
mod build_preview;
mod client_module;
mod client_packages;
mod command_line;
mod command_panel;
mod connection;
mod dock_click;
mod engine_text;
mod error;
mod gather_click;
mod hud;
mod input_map;
mod input_mode;
mod inventory_bar;
mod item_names;
mod learn_keys;
mod link_watch;
mod local_keys;
mod name_texts;
mod orders;
mod own_player;
mod perk_panel;
mod placing;
mod pointer;
mod rally_click;
mod report;
mod right_click;
mod selecting;
mod selection;
mod shop_panel;
#[cfg(test)]
mod test_app;
mod ui_style;
mod use_click;
mod view;
mod window_frame;

/// What the terminal shows when `RUST_LOG` does not say: the renderer's validation layers report
/// through `wgpu_hal`, loudly, in debug builds.
const TERMINAL_FILTER: &str = "info,wgpu=error,wgpu_hal=off,naga=warn";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and the network stack's rollbacks; of the rest, `info` and above, without the
/// renderer's validation layers.
const FILE_FILTER: &str = "info,campfire_client=debug,campfire_host=debug,campfire_net=debug,campfire_script=debug,\
                           campfire_net_prediction=debug,wgpu=warn,wgpu_hal=off,naga=warn";

/// Runs the client as its command line asks, and gives the exit code it ends with.
pub fn run() -> ExitCode {
    let log = Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    match launch(&log.tap) {
        Ok(code) => code,
        Err(error) => {
            if !matches!(error, ClientLaunchError::CommandLine(_)) {
                error!(error = %ErrorReport::of(&error), "the client does not start");
            }
            ExitCode::from(error.status())
        }
    }
}

/// Starts the client and runs it, its console's events taken from `tap`: the exit code of its
/// end, or the refusal that ended its start.
fn launch(tap: &LogTap) -> Result<ExitCode, ClientLaunchError> {
    let line = Logging::command_line::<CommandLine>(env::args_os())
        .map_err(ClientLaunchError::CommandLine)?;
    let args = Args::try_from(line)?;
    let packages = Arc::new(ModePackages::at(
        &PackageDir::new(&args.mode),
        args.map.as_ref(),
    )?);
    let script = args.bot.as_ref().map(OrderFile::read).transpose()?;
    let script = script
        .map(|script| BotScript::new(&script, &BotNames::new(packages.load_books())))
        .transpose()?;
    let main = Arc::new(main_key(&args)?);
    let role = args.role.resolve(&packages)?;
    // Opened before the server, so its tool hears the client at once.
    let stdio = args
        .console
        .map(|ConsoleArg::Stdio| ConsoleStdio::open(GameRole::Client, tap));
    let seen = stdio.as_ref().map(ConsoleStdio::seen);
    let mut connection = Connection::open(&args, role, &packages, seen)?;
    let end = match script {
        Some(_) => ClientEnd::Bot,
        None => ClientEnd::Window {
            local: connection.pace().cloned(),
            data: connection.data().map(|data| data.layout().clone()),
            mode_dir: path::absolute(&args.mode)
                .expect("the mode loaded from its directory, so its path resolves"),
            camera: args.server.camera().copied(),
        },
    };
    let sim_client = SimClientPlugin {
        main,
        session: SessionSigner::new(Signer::new(
            Os::SYSTEM.entropy.keypair(),
            Os::SYSTEM.entropy,
        )),
        server: connection.pin(),
        role,
        hosting: connection.hosting(),
        packages: Arc::clone(&packages),
        os: Os::SYSTEM,
        data: connection.data(),
        bot: script,
    };
    sim_client.check()?;

    let mut app = App::new();
    end.add(&mut app, &packages);
    app.add_plugins((sim_client, LinkWatchPlugin));
    if let Some(stdio) = stdio {
        app.insert_resource(stdio);
        app.init_resource::<ClientConsole>();
    }
    let client = SimClientPlugin::spawn(app.world_mut());
    connection.link(&mut app, client)?;
    app.world_mut().trigger(Connect { entity: client });
    let exit = app.run();
    // Dropped, a local server ends the session and publishes its log.
    drop(connection);
    Ok(exit_code(exit))
}

/// The signer of the player's main key: the key file's, made when missing, or a new one with no
/// file.
fn main_key(args: &Args) -> Result<Signer, KeyFileError> {
    let entropy = Os::SYSTEM.entropy;
    let Some(path) = &args.key else {
        return Ok(Signer::new(entropy.keypair(), entropy));
    };
    KeyFile::read_or_create(&SecretFile::at(path.clone()), entropy)
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::pointer::unit_picking;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        unit_picking::bench::picking(c);
    }
}
