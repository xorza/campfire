//! Game client: joins a session over QUIC, predicts the player's own avatar, and draws the
//! match; a left click or drag selects the player's own units, and a right click orders them.
//! With `--bot <orders file>`, it opens no window and renders nothing, and plays the file's
//! `OrderScript` instead. With `--key <file>`, the player's main key is the file's, made when
//! missing; without it, the player is a new key each run. With `--data <directory>`, the client
//! writes the newest receipt of its session there; without it, it writes none. With `--local`, it
//! plays the mode alone on a local server, a thread of its own process, whose data goes in
//! `server` under the data directory, `--data` then needed; each `--server-bot <slot>=<orders
//! file>` gives the server a bot there, and every other slot but the player's is open. Space
//! pauses a local match and plays it on, minus and equals, or the keypad's minus and plus, step
//! its speed through 0.5, 1, 2 and 4 times, F5 saves it, and F9 loads its latest save.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info`, and the renderer's warnings, by
//! default). With `CAMPFIRE_LOG` set to a path, they also go there as JSON lines, filtered by
//! `CAMPFIRE_LOG_FILTER` (Campfire's `debug` by default) with the directives of
//! `CAMPFIRE_LOG_FILTER_EXTRA` added.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use bevy::DefaultPlugins;
use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin, TaskPoolPlugin};
use bevy::asset::AssetApp;
use bevy::asset::io::AssetSourceBuilder;
use bevy::log::LogPlugin;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use bevy::window::{Window, WindowPlugin};
use campfire_common::ExitStatus;
use campfire_log::{ErrorReport, Logging};
use campfire_net::{
    ClientDir, JoinRole, KeyFile, NetProtocol, OrderScript, Os, Pace, ProcessExit, SimClient,
    WatchRequest,
};
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_protocol::RandomKey;
use campfire_sim::{Capability, TickRate};
use campfire_store::SecretFile;
use campfire_values::{Team, TeamSet};
use lightyear::prelude::Connect;
use secp256k1::Keypair;
use tracing::error;

use crate::args::Args;
use crate::bot::Bot;
use crate::connection::Connection;
use crate::hud::Hud;
use crate::inventory_bar::InventoryBar;
use crate::item_names::ItemNames;
use crate::link_watch::LinkWatch;
use crate::local_keys::LocalKeys;
use crate::orders::Orders;
use crate::selecting::Selecting;
use crate::shop_panel::ShopPanel;
use crate::view::View;
use crate::view::package_source::PackageSource;
use crate::view::player_camera::PlayerCamera;
use crate::view::terrain_view::TerrainView;
use crate::view::unit_models::UnitModels;

mod args;
mod bot;
mod connection;
mod hud;
mod inventory_bar;
mod item_names;
mod link_watch;
mod local_keys;
mod orders;
mod own_player;
mod pointer;
mod selecting;
mod selection;
mod shop_panel;
mod ui_style;
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
    let _log = Logging {
        terminal: TERMINAL_FILTER,
        file: FILE_FILTER,
    }
    .start();
    let args = match Logging::command_line(env::args_os()) {
        Ok(line) => Args::of(line),
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
    let main_key = match main_key(&args) {
        Ok(key) => key,
        Err(code) => return code,
    };
    let data = match &args.data {
        None => None,
        Some(path) => match ClientDir::open(path) {
            Ok(data) => Some(Arc::new(data)),
            Err(error) => {
                error!(error = %ErrorReport::of(&error), "the data directory does not open");
                return ExitCode::from(ExitStatus::Failure);
            }
        },
    };
    let watch = match watched_teams(&args, &packages) {
        Ok(watch) => watch,
        Err(code) => return code,
    };
    let pace = Arc::new(Pace::default());
    let mut connection = match Connection::open(&args, data.as_deref(), &packages, &pace) {
        Ok(connection) => connection,
        Err(code) => return code,
    };
    let pin = connection.pin();
    let tick = TickRate::new(pin.tick_hz).length();
    let sim_client = SimClient {
        main_key,
        session_key: RandomKey::generate(|bytes| Os::SYSTEM.entropy.fill(bytes)),
        server: pin,
        role: if args.spectate {
            JoinRole::Watch
        } else {
            JoinRole::Play
        },
        local: matches!(connection, Connection::Local(_)),
        packages: Arc::clone(&packages),
        os: Os::SYSTEM,
        data,
    };
    if let Err(error) = sim_client.check() {
        error!(error = %ErrorReport::of(&error), "the mode does not run at the listing's rate");
        return ExitCode::from(ExitStatus::Usage);
    }

    let mut app = App::new();
    if let Some(teams) = watch {
        app.insert_resource(WatchRequest(teams));
    }
    add_ends(
        &mut app,
        script,
        sim_client.local.then_some(&pace),
        &packages,
    );
    app.add_plugins((sim_client, LinkWatch));
    let client = SimClient::spawn(app.world_mut());
    if let Err(code) = connection.link(&mut app, client, &pace, tick) {
        return code;
    }
    app.world_mut().trigger(Connect { entity: client });
    let exit = app.run();
    // Dropped, a local server ends the session and publishes its log.
    drop(connection);
    ProcessExit::code(exit)
}

/// Adds the client's own end: a bot playing `script`, with no window, or the view, the HUD, the
/// selection, the orders, the shop panel and the inventory bar, with the keys of a local match
/// that `pace` follows, and the models and item names of `packages`.
fn add_ends(
    app: &mut App,
    script: Option<OrderScript>,
    pace: Option<&Arc<Pace>>,
    packages: &Arc<ModePackages>,
) {
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
    // Bevy builds its asset sources as its asset plugin is added, so the packages' goes first.
    // Every reader the builder makes shares the one source's bound on the models that load.
    let source = PackageSource::of(packages);
    app.register_asset_source(
        PackageSource::NAME,
        AssetSourceBuilder::new(move || Box::new(source.clone())),
    );
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
        View,
        PlayerCamera,
        UnitModels {
            packages: Arc::clone(packages),
        },
        TerrainView {
            packages: Arc::clone(packages),
        },
        Hud,
        Selecting,
        Orders,
    ));
    // The item names, the shop and the inventory read the items' books, which a mode of no
    // `items` has none of.
    if packages.manifest().capabilities.contains(Capability::Items) {
        app.add_plugins((
            ItemNames {
                packages: Arc::clone(packages),
            },
            ShopPanel,
            InventoryBar,
        ));
    }
}

/// The packages of the mode the arguments name; the exit code when they do not load.
fn load_mode(args: &Args) -> Result<ModePackages, ExitCode> {
    let mode = PackageDir::new(&args.mode);
    let packages = mode.choose_map(args.map.clone())
        .and_then(|map| mode.mode_packages(&map))
        .map_err(|error| {
        error!(mode = %args.mode.display(), error = %ErrorReport::of(&error), "the mode does not load");
        ExitCode::from(ExitStatus::Failure)
    })?;
    Ok(packages)
}

/// The player's main key: the key file's, made when missing, or a new one with no file; the exit
/// code when the file does not read.
fn main_key(args: &Args) -> Result<Keypair, ExitCode> {
    let Some(path) = &args.key else {
        return Ok(RandomKey::generate(|bytes| Os::SYSTEM.entropy.fill(bytes)));
    };
    let file = SecretFile::at(path.clone());
    KeyFile::read_or_create(&file, Os::SYSTEM.entropy).map_err(|error| {
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
