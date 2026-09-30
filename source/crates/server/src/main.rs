//! Headless game server: opens a session of a mode on an address, lets its players join over
//! WebTransport, runs the match, and writes the session log once every player left.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
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
use bevy_ecs::system::{Commands, Local, Query, Res};
use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_net::{
    JoinRefused, Joined, Lobby, LobbySetup, MatchClock, NetProtocol, PlayerLink, SimServer,
};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{CertificateHash, SeedChain};
use campfire_runner::Session;
use campfire_sim::TickRate;
use lightyear::prelude::server::{RawServer, ServerPlugins, Start, WebTransportServerIo};
use lightyear::prelude::{Connected, Identity, LinkOf, LocalAddr, ReplicationSender};

/// How often the app loop runs: often enough that no fixed tick waits long for its frame.
const FRAME: Duration = Duration::from_millis(2);

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let (Some(mode), Some(address), None) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: campfire-server <mode package directory> <address, as 0.0.0.0:4433>");
        return ExitCode::from(2);
    };
    let mode = PathBuf::from(mode);
    let Some(address) = address
        .to_str()
        .and_then(|text| text.parse::<SocketAddr>().ok())
    else {
        eprintln!("{}: not a socket address", address.display());
        return ExitCode::from(2);
    };
    let packages = match ModePackages::from_dir(&mode) {
        Ok(packages) => packages,
        Err(error) => {
            eprintln!("{}: {error}", mode.display());
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
    println!("campfire server on {address}, waiting for {players} players");
    println!("certificate hash: {certificate}");
    println!("server key: {key}");
    println!(
        "join with: campfire-client {} <this machine's LAN address>:{} {certificate} {key}",
        mode.display(),
        address.port()
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
    app.add_observer(
        |_: On<'_, '_, Add, Joined>, lobby: Option<Res<'_, Lobby>>| {
            if let Some(lobby) = lobby {
                println!("a player joined: {} of {}", lobby.joined(), lobby.players());
            }
        },
    );
    app.add_observer(
        |refused: On<'_, '_, Add, JoinRefused>, links: Query<'_, '_, &JoinRefused>| {
            if let Ok(JoinRefused(error)) = links.get(refused.entity) {
                println!("refused a player: {error}");
            }
        },
    );
    app.add_systems(Update, (report_start, end_when_everyone_left));
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

/// Prints the start of the match.
fn report_start(clock: Option<Res<'_, MatchClock>>, mut started: Local<'_, bool>) {
    if clock.is_some() && !*started {
        *started = true;
        println!("the match started");
    }
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
    let name = format!("{id}.campfire-log");
    let written = fs::write(Path::new(&name), &bytes);
    world.write_message(AppExit::Success);
    match written {
        Ok(()) => {
            println!("every player left; the session log is {name}");
            println!("final state hash: {hash}");
            println!("verify with: campfire-verifier <packages directory> {name}");
            println!("the verifier prints the same hash when the log verifies");
        }
        Err(error) => eprintln!("{name}: {error}"),
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
