//! A local server on its own thread, which a client app in the same process plays through
//! in-process channels: it opens a session in its data directory, the client's player takes slot
//! 0 and plays, saves and loads the save, the server starting again and the client taking its new
//! link; once the server drops, the session ends and its log is published.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use bevy_app::{App, TaskPoolPlugin};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_net::{
    Clock, Hosting, JoinRole, JoinState, LocalRelink, LocalServer, LocalServerSetup, NetProtocol,
    Os, Pace, PendingSaves, SaveCommand, ServerDir, ServerPin, SessionDir, SimClient,
};
use campfire_package_disk::internals::TestMode;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Entropy, Outcome, SessionLog};
use campfire_runner::TermsError;
use campfire_store::internals::Scratch;
use lightyear::prelude::Connect;

/// How long the client gets to join and play.
const DEADLINE: Duration = Duration::from_secs(10);

#[test]
fn a_client_plays_a_local_server_which_publishes_the_log_as_it_drops() {
    let scratch = Scratch::new();
    let packages = Arc::new(TestMode::Lane.packages());
    let pace = Arc::new(Pace::default());
    let mut server = LocalServer::start(LocalServerSetup {
        packages: Arc::clone(&packages),
        data: scratch.path("data"),
        bots: Vec::new(),
        pace,
        os: Os {
            clock: Clock::manual(TestKey::NOW),
            entropy: Entropy::Fixed(7),
        },
    })
    .unwrap();

    let keys = TestKey::player(0);
    let mut client = App::new();
    client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
    let setup = SimClient {
        main_key: keys.main,
        session_key: keys.session,
        server: server.pin(),
        role: JoinRole::Play,
        hosting: Hosting::Local,
        packages,
        os: Os {
            clock: Clock::manual(TestKey::NOW),
            entropy: Entropy::Fixed(4),
        },
        data: None,
    };
    // The lane runs at 30 Hz alone: a server pinned at 20 Hz fails the check, as a listing at a
    // rate the mode does not run at does.
    assert_eq!(setup.check(), Ok(()));
    let elsewhere = ServerPin {
        tick_hz: NonZeroU32::new(20).unwrap(),
        ..setup.server
    };
    let at_20 = SimClient {
        server: elsewhere,
        packages: Arc::clone(&setup.packages),
        os: setup.os.clone(),
        data: None,
        ..setup
    };
    assert_eq!(at_20.check(), Err(TermsError::TickRate(elsewhere.tick_hz)));
    client.add_plugins(setup);
    client.add_plugins(LocalRelink {
        relinks: server.relinks(),
    });
    client.finish();
    client.cleanup();
    let entity = SimClient::spawn(client.world_mut());
    client
        .world_mut()
        .entity_mut(entity)
        .insert(server.take_link());
    client.world_mut().trigger(Connect { entity });
    let start = Instant::now();
    let frame = |client: &mut App| {
        assert!(start.elapsed() < DEADLINE, "the client did not play on");
        client.update();
        thread::sleep(NetProtocol::FRAME);
    };
    let playing = |client: &App| {
        client
            .world()
            .resource::<JoinState>()
            .playing_clock()
            .is_some()
    };
    let mut played = 0;
    while played < 30 {
        frame(&mut client);
        played += usize::from(playing(&client));
    }

    // A save, then a load of it: the server starts again on its data, and the client loses its
    // link, takes the new one, and plays on.
    let save = |client: &mut App, command| {
        client
            .world_mut()
            .resource_mut::<PendingSaves>()
            .push(command);
    };
    save(&mut client, SaveCommand::Save);
    for _ in 0..30 {
        frame(&mut client);
    }
    save(&mut client, SaveCommand::LoadLatest);
    while playing(&client) {
        frame(&mut client);
    }
    let mut played = 0;
    while played < 30 {
        frame(&mut client);
        played += usize::from(playing(&client));
    }

    // Dropped, the server ends the session, aborted as the mode did not end the match, and
    // publishes its log.
    drop(server);
    let stopped = ServerDir::open(&scratch.path("data")).unwrap();
    assert!(SessionDir::find(&stopped).unwrap().is_none());
    let logs = scratch.names("data/logs");
    assert_eq!(logs.len(), 1);
    let log = SessionLog::decode(&scratch.read(format!("data/logs/{}", logs[0]))).unwrap();
    assert_eq!(
        log.result().map(|result| result.outcome),
        Some(Outcome::Aborted)
    );
    let saves: Vec<_> = log.checkpoints().collect();
    assert_eq!(saves.len(), 1);
    assert!(log.next_tick() > saves[0].tick);
}
