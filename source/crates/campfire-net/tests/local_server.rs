//! A local server on its own thread, which a client app in the same process plays through
//! in-process channels: it opens a session in its data directory, the client's player takes slot
//! 0 and plays, and once the server drops, the session ends and its log is published.

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use bevy_app::{App, TaskPoolPlugin};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_net::{
    JoinState, LocalServer, LocalServerSetup, NetProtocol, Pace, SessionDir, SimClient,
};
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{Outcome, SessionLog};
use campfire_sim::TickRate;
use lightyear::prelude::client::{ClientPlugins, RawClient};
use lightyear::prelude::{Client, Connect, PredictionManager, ReplicationReceiver};

use crate::Scratch;

/// How long the client gets to join and play.
const DEADLINE: Duration = Duration::from_secs(10);

fn keypair(secret: u8) -> Keypair {
    Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_byte_array(&[secret; 32]).unwrap(),
    )
}

#[test]
fn a_client_plays_a_local_server_which_publishes_the_log_as_it_drops() {
    let data = Scratch::new("local-server");
    let lane = || ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).unwrap();
    let packages = lane();
    let tick = TickRate::new(packages.manifest().tick_hz.default()).length();
    let pace = Arc::new(Pace::default());
    let mut server = LocalServer::start(LocalServerSetup {
        packages: lane(),
        data: data.0.clone(),
        bots: Vec::new(),
        pace,
        clock: || 1_700_000_000,
        entropy: |bytes| bytes.fill(7),
    })
    .unwrap();

    let mut client = App::new();
    client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
    client.add_plugins(ClientPlugins {
        tick_duration: tick,
    });
    client.add_plugins((
        NetProtocol,
        SimClient {
            main_key: keypair(1),
            session_key: keypair(2),
            server: server.pin(),
            packages: Arc::new(packages),
            clock: || 1_700_000_000,
            entropy: |bytes| bytes.fill(4),
            data: None,
        },
    ));
    client.insert_resource(PredictionManager::default());
    client.finish();
    client.cleanup();
    let entity = client
        .world_mut()
        .spawn((Client, RawClient, ReplicationReceiver, server.take_link()))
        .id();
    client.world_mut().trigger(Connect { entity });
    let start = Instant::now();
    let mut played = 0;
    while played < 30 {
        assert!(start.elapsed() < DEADLINE, "the client did not play");
        client.update();
        if client.world().resource::<JoinState>().clock().is_some() {
            played += 1;
        }
        thread::sleep(Duration::from_millis(2));
    }

    // Dropped, the server ends the session, aborted as the mode did not end the match, and
    // publishes its log.
    drop(server);
    assert!(SessionDir::find(&data.0).unwrap().is_none());
    let logs: Vec<_> = fs::read_dir(data.0.join("logs")).unwrap().collect();
    assert_eq!(logs.len(), 1);
    let log = SessionLog::decode(&fs::read(logs[0].as_ref().unwrap().path()).unwrap()).unwrap();
    assert_eq!(
        log.result().map(|result| result.outcome),
        Some(Outcome::Aborted)
    );
    assert!(log.next_tick().get() > 0);
}
