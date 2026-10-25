//! The game client, which its binary runs: its command line, its connection, and what its window
//! draws and takes from the player.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::path;
use std::process::ExitCode;
use std::sync::Arc;

use bevy::app::App;
use campfire_bots::{BotNames, BotScript};
use campfire_common::{ErrorReport, ExitStatus};
use campfire_host::{KeyFile, OrderFile, Os, SessionSigner, exit_code};
use campfire_logging::Logging;
use campfire_net::SimClientPlugin;
use campfire_net_connection::Connect;
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_protocol::Signer;
use campfire_store::SecretFile;
use tracing::error;

use crate::client_module::ClientEnd;
use crate::command_line::{Args, CommandLine};
use crate::connection::Connection;
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
    let _log = Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    let args = match Logging::command_line::<CommandLine>(env::args_os()).map(Args::try_from) {
        Ok(Ok(args)) => args,
        Ok(Err(error)) => {
            error!(error = %error, "the command line is refused");
            return ExitCode::from(ExitStatus::Usage);
        }
        Err(status) => return ExitCode::from(status),
    };
    let packages = match load_mode(&args) {
        Ok(packages) => Arc::new(packages),
        Err(code) => return code,
    };
    let script = match args.bot.as_ref().map(OrderFile::read).transpose() {
        Ok(script) => script,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the orders file does not read");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let script = match script
        .map(|script| BotScript::new(&script, &BotNames::new(packages.load_books())))
        .transpose()
    {
        Ok(script) => script,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the orders name what the mode does not have");
            return ExitCode::from(ExitStatus::Usage);
        }
    };
    let main = match main_key(&args) {
        Ok(signer) => Arc::new(signer),
        Err(code) => return code,
    };
    let role = match args.role.resolve(&packages) {
        Ok(role) => role,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the mode has no such team");
            return ExitCode::from(ExitStatus::Usage);
        }
    };
    let mut connection = match Connection::open(&args, role, &packages) {
        Ok(connection) => connection,
        Err(code) => return code,
    };
    let end = match script {
        Some(_) => ClientEnd::Bot,
        None => ClientEnd::Window {
            local: connection.pace().cloned(),
            data: connection.data().map(|data| data.layout().clone()),
            mode_dir: path::absolute(&args.mode)
                .expect("the mode loaded from its directory, so its path resolves"),
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
    if let Err(error) = sim_client.check() {
        error!(error = %ErrorReport::of(&error), "the mode does not run at the listing's rate");
        return ExitCode::from(ExitStatus::Usage);
    }

    let mut app = App::new();
    end.add(&mut app, &packages);
    app.add_plugins((sim_client, LinkWatchPlugin));
    let client = SimClientPlugin::spawn(app.world_mut());
    if let Err(code) = connection.link(&mut app, client) {
        return code;
    }
    app.world_mut().trigger(Connect { entity: client });
    let exit = app.run();
    // Dropped, a local server ends the session and publishes its log.
    drop(connection);
    exit_code(exit)
}

/// The packages of the mode the arguments name; the exit code when they do not load.
fn load_mode(args: &Args) -> Result<ModePackages, ExitCode> {
    let mode = PackageDir::new(&args.mode);
    let packages = ModePackages::at(&mode, args.map.as_ref()).map_err(|error| {
        error!(mode = %args.mode.display(), error = %ErrorReport::of(&error), "the mode does not load");
        ExitCode::from(ExitStatus::Failure)
    })?;
    Ok(packages)
}

/// The signer of the player's main key: the key file's, made when missing, or a new one with no
/// file; the exit code when the file does not read.
fn main_key(args: &Args) -> Result<Signer, ExitCode> {
    let entropy = Os::SYSTEM.entropy;
    let Some(path) = &args.key else {
        return Ok(Signer::new(entropy.keypair(), entropy));
    };
    let file = SecretFile::at(path.clone());
    KeyFile::read_or_create(&file, entropy).map_err(|error| {
        error!(key = %file, error = %ErrorReport::of(&error), "the key file does not read");
        ExitCode::from(ExitStatus::Failure)
    })
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
