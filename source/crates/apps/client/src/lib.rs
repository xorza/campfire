//! The game client, which its binary runs: its command line, its connection, and what its window
//! draws and takes from the player.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use bevy::app::App;
use campfire_bots::{BotNames, BotScript, OrderScript};
use campfire_common::{ErrorReport, ExitStatus};
use campfire_log::Logging;
use campfire_net::{
    Hosting, KeyFile, Os, Pace, SessionSigner, SimClientPlugin, WatchRequest, exit_code,
};
use campfire_net_connection::Connect;
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_protocol::Signer;
use campfire_sim::MatchRate;
use campfire_store::SecretFile;
use campfire_values::{Team, TeamSet};
use tracing::error;

use crate::client_module::ClientEnd;
use crate::command_line::{Args, CommandLine};
use crate::connection::Connection;
use crate::link_watch::LinkWatchPlugin;

mod action_names;
mod bot;
mod build_preview;
mod client_module;
mod client_packages;
mod command_line;
mod command_panel;
mod connection;
mod dock_click;
mod engine_text;
mod hud;
mod input_map;
mod input_mode;
mod inventory_bar;
mod item_names;
mod link_watch;
mod local_keys;
mod orders;
mod own_player;
mod placing;
mod pointer;
mod selecting;
mod selection;
mod shop_panel;
#[cfg(test)]
mod test_app;
mod ui_style;
mod view;

/// What the terminal shows when `RUST_LOG` does not say: the renderer's validation layers report
/// through `wgpu_hal`, loudly, in debug builds.
const TERMINAL_FILTER: &str = "info,wgpu=error,wgpu_hal=off,naga=warn";
/// What the log file holds when `CAMPFIRE_LOG_FILTER` does not say: Campfire's messages down to
/// `debug`, and the network stack's rollbacks; of the rest, `info` and above, without the
/// renderer's validation layers.
const FILE_FILTER: &str = "info,campfire_client=debug,campfire_net=debug,campfire_script=debug,\
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
    let script = match args.bot.as_deref().map(OrderScript::read).transpose() {
        Ok(script) => script,
        Err(error) => {
            error!(error = %ErrorReport::of(&error), "the orders file does not read");
            return ExitCode::from(ExitStatus::Failure);
        }
    };
    let script = match script
        .map(|script| BotScript::new(&script, &BotNames::new(&packages.name_books())))
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
    let watch = match watched_teams(&args, &packages) {
        Ok(watch) => watch,
        Err(code) => return code,
    };
    let pace = Arc::new(Pace::default());
    let mut connection = match Connection::open(&args, &packages, &pace) {
        Ok(connection) => connection,
        Err(code) => return code,
    };
    let pin = connection.pin();
    let rate = MatchRate::new(pin.tick_hz).rate();
    let sim_client = SimClientPlugin {
        main,
        session: SessionSigner::new(Signer::new(
            Os::SYSTEM.entropy.keypair(),
            Os::SYSTEM.entropy,
        )),
        server: pin,
        role: args.role,
        hosting: match connection {
            Connection::Local { .. } => Hosting::Local,
            Connection::Remote { .. } => Hosting::Remote,
        },
        packages: Arc::clone(&packages),
        os: Os::SYSTEM,
        data: connection.data(),
    };
    if let Err(error) = sim_client.check() {
        error!(error = %ErrorReport::of(&error), "the mode does not run at the listing's rate");
        return ExitCode::from(ExitStatus::Usage);
    }

    let mut app = App::new();
    if let Some(teams) = watch {
        app.insert_resource(WatchRequest(teams));
    }
    let end = match script {
        Some(script) => ClientEnd::Bot(script),
        None => ClientEnd::Window {
            local: (sim_client.hosting == Hosting::Local).then(|| Arc::clone(&pace)),
        },
    };
    end.add(&mut app, &packages);
    app.add_plugins((sim_client, LinkWatchPlugin));
    let client = SimClientPlugin::spawn(app.world_mut());
    if let Err(code) = connection.link(&mut app, client, &pace, rate) {
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
    let packages = mode.mode_packages(args.map.as_ref()).map_err(|error| {
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

/// The teams a spectator watches, as `--watch` names them in the mode `packages` holds; none to
/// watch every team's view; the exit code for a name no team of the mode has.
fn watched_teams(args: &Args, packages: &ModePackages) -> Result<Option<TeamSet>, ExitCode> {
    if args.watch.is_empty() {
        return Ok(None);
    }
    let teams = &packages.manifest().teams;
    let mut watched = Vec::with_capacity(args.watch.len());
    for name in &args.watch {
        let Some(index) = teams.iter().position(|team| team.name == *name) else {
            error!(team = %name, "the mode has no such team");
            return Err(ExitCode::from(ExitStatus::Usage));
        };
        watched.push(Team::new(
            u8::try_from(index).expect("a team's index fits u8"),
        ));
    }
    Ok(Some(watched.into_iter().collect()))
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
