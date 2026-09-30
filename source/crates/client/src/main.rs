//! Game client: joins a session over WebTransport, predicts the player's own hero, and draws the
//! match as capsules on the ground; a right click walks the hero there.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::env;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::DefaultPlugins;
use bevy::app::{App, AppExit, PluginGroup, Update};
use bevy::ecs::system::{Local, Res};
use bevy::window::{Window, WindowPlugin};
use campfire_net::{ClientMode, JoinState, NetProtocol, ServerPin, SimClient};
use campfire_package::ModePackages;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_sim::TickRate;
use lightyear::prelude::client::{ClientPlugins, RawClient, WebTransportClientIo};
use lightyear::prelude::{
    Client, Connect, LocalAddr, PeerAddr, PredictionManager, ReplicationReceiver,
};

use crate::orders::Orders;
use crate::view::View;

mod orders;
mod view;

/// What the command line names: the mode to play, and the server as its listing gives it.
#[derive(Debug)]
struct Args {
    mode: PathBuf,
    address: SocketAddr,
    certificate: CertificateHash,
    server_key: XOnlyPublicKey,
}

fn main() -> ExitCode {
    let args = match Args::parse(env::args_os().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: campfire-client <mode package directory> <server address> \
                 <certificate hash> <server key>"
            );
            return ExitCode::from(2);
        }
    };
    let packages = match ModePackages::from_dir(&args.mode) {
        Ok(packages) => packages,
        Err(error) => {
            eprintln!("{}: {error}", args.mode.display());
            return ExitCode::FAILURE;
        }
    };
    let mode = ClientMode::of(&packages);
    let tick = TickRate::new(mode.tick_hz).length();

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Campfire".to_owned(),
            ..Window::default()
        }),
        ..WindowPlugin::default()
    }));
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
        View { tick },
        Orders,
    ));
    app.insert_resource(PredictionManager::default());
    app.add_systems(Update, report_join);
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
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Args, String> {
        let (Some(mode), Some(address), Some(certificate), Some(server_key), None) = (
            args.next(),
            args.next(),
            args.next(),
            args.next(),
            args.next(),
        ) else {
            return Err("four arguments are needed".to_owned());
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

/// Prints each change of the join.
fn report_join(state: Res<'_, JoinState>, mut reported: Local<'_, Option<JoinState>>) {
    if *reported == Some(*state) {
        return;
    }
    *reported = Some(*state);
    match *state {
        JoinState::Waiting => println!("connecting"),
        JoinState::Joined => println!("joined; the match starts when every player joined"),
        JoinState::Refused(mismatch) => println!("did not join: {mismatch}"),
    }
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
