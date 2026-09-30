//! Headless game server: opens a session of a mode on an address, lets its players join over
//! WebTransport, runs the match, and writes the session log once every player left.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info` by default). With `CAMPFIRE_LOG` set
//! to a path, they also go there as JSON lines, filtered by `CAMPFIRE_LOG_FILTER` (Campfire's
//! `debug` by default).

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::fs;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy_app::{App, AppExit, ScheduleRunnerPlugin, TaskPoolPlugin, Update};
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_log::{LogEvent, Logging};
use campfire_net::{
    Listening, Lobby, LobbySetup, MatchClock, NetProtocol, PlayerLink, SessionWritten, SimServer,
};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{CertificateHash, SeedChain};
use campfire_runner::Session;
use campfire_sim::TickRate;
use lightyear::prelude::server::{RawServer, ServerPlugins, Start, WebTransportServerIo};
use lightyear::prelude::{Connected, Identity, LinkOf, Linked, LocalAddr, ReplicationSender};
use tracing::{error, info};

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
    let mut args = env::args_os().skip(1);
    let (Some(mode), Some(address), None) = (args.next(), args.next(), args.next()) else {
        error!("usage: campfire-server <mode package directory> <address, as 0.0.0.0:4433>");
        return ExitCode::from(2);
    };
    let mode = PathBuf::from(mode);
    let Some(address) = address
        .to_str()
        .and_then(|text| text.parse::<SocketAddr>().ok())
    else {
        error!(address = %address.display(), "not a socket address");
        return ExitCode::from(2);
    };
    let packages = match ModePackages::from_dir(&mode) {
        Ok(packages) => packages,
        Err(error) => {
            error!(mode = %mode.display(), %error, "the mode does not load");
            return ExitCode::FAILURE;
        }
    };
    let identity = Identity::self_signed(["localhost"]).expect("a fixed name is a valid SAN");
    let certificate =
        CertificateHash::new(*identity.certificate_chain().as_slice()[0].hash().as_ref());
    let server_key = keypair();
    let players = packages
        .manifest()
        .teams
        .iter()
        .map(|team| team.slots as usize)
        .sum();
    let lobby = Lobby::new(LobbySetup {
        packages,
        server_key: server_key.x_only_public_key().0.serialize(),
        seed_chain: SeedChain::new(random(), NonZeroU32::MIN),
        certificate,
        players,
        clock: unix_now,
        entropy: fill,
    });
    let tick = TickRate::new(lobby.terms().tick_hz).length();

    let key = server_key.x_only_public_key().0;
    info!(
        session = %lobby.terms().session_id(),
        %address,
        players,
        mode = %mode.display(),
        "opened a session"
    );

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
    app.insert_resource(lobby);
    app.add_observer(
        |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
            commands.entity(added.entity).insert(ReplicationSender);
        },
    );
    let listening = Listening {
        certificate,
        server_key: key,
        join: format!(
            "campfire-client {} <this machine's LAN address>:{} {certificate} {key}",
            mode.display(),
            address.port()
        ),
    };
    app.add_observer(
        move |added: On<'_, '_, Add, Linked>, servers: Query<'_, '_, (), With<RawServer>>| {
            if servers.contains(added.entity) {
                listening.log();
            }
        },
    );
    app.add_systems(Update, end_when_everyone_left);
    let server = app
        .world_mut()
        .spawn((
            RawServer,
            WebTransportServerIo {
                certificate: identity,
            },
            LocalAddr(address),
        ))
        .id();
    app.world_mut().trigger(Start { entity: server });
    app.run();
    ExitCode::SUCCESS
}

/// Once the match started and no player is connected any more, reveals the seed, writes the
/// session log into the working directory and exits.
fn end_when_everyone_left(world: &mut World) {
    if !world.contains_resource::<MatchClock>() {
        return;
    }
    let mut connected = world.query_filtered::<(), (With<PlayerLink>, With<Connected>)>();
    if connected.iter(world).next().is_some() {
        return;
    }
    let mut session = world.resource_mut::<Session>();
    session.reveal_seed();
    let session = world.resource::<Session>();
    let hash = session.state_hash(world);
    let mut bytes = Vec::new();
    session.log().encode(&mut bytes);
    let id = session.log().header().terms.session_id();
    let file = PathBuf::from(format!("{id}.campfire-log"));
    let written = fs::write(&file, &bytes);
    world.write_message(AppExit::Success);
    match written {
        Ok(()) => SessionWritten {
            session: id,
            file,
            hash,
        }
        .log(),
        Err(error) => error!(file = %file.display(), %error, "could not write the session log"),
    }
}

/// A fresh server key.
fn keypair() -> Keypair {
    loop {
        if let Ok(secret) = SecretKey::from_byte_array(&random()) {
            return Keypair::from_secret_key(&Secp256k1::new(), &secret);
        }
    }
}

fn random() -> [u8; 32] {
    let mut bytes = [0; 32];
    fill(&mut bytes);
    bytes
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
