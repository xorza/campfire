//! A local server on its own thread, which a client app in the same process plays through
//! in-process channels: it opens a session in its data directory, the client's player takes slot
//! 0 and plays, saves and loads the save, the server starting again and the client taking its new
//! link; once the server drops, the session ends and its log is published.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::local::local_server::{LocalServer, LocalServerSetup};
use crate::sim_client::{PendingSaves, SimClientPlugin};
use bevy_app::App;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_common::Ticks;
use campfire_host::ClientLayout;
use campfire_host::Hosting;
use campfire_host::JoinRole;
use campfire_host::JoinState;
use campfire_host::Os;
use campfire_host::Pace;
use campfire_host::ReportKey;
use campfire_host::SaveCommand;
use campfire_host::ServerDir;
use campfire_host::ServerPin;
use campfire_host::SessionDir;
use campfire_host::SessionSigner;
use campfire_host::UnixClock;
use campfire_net_connection::Connect;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Entropy, Outcome, SessionLog};
use campfire_runner::{Replay, SessionWorld, TermsError};
use campfire_sim::{SimTick, SimWorld};
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;

/// How long the client gets to join and play.
const DEADLINE: Duration = Duration::from_secs(10);

/// The time between two frames of the client, which its test runs by hand.
const FRAME: Duration = Duration::from_millis(2);

#[test]
fn a_client_plays_a_local_server_which_publishes_the_log_as_it_drops() {
    let test_dir = TestDir::new();
    let packages = Arc::new(TestMode::Lane.packages());
    let pace = Arc::new(Pace::default());
    let mut server = LocalServer::start(LocalServerSetup {
        packages: Arc::clone(&packages),
        data: test_dir.path("data"),
        bots: Vec::new(),
        options: Vec::new(),
        player_slot_picks: Vec::new(),
        role: JoinRole::Play,
        pace,
        os: Os {
            clock: UnixClock::manual(TestKey::NOW),
            entropy: Entropy::Fixed(7),
        },
        reports: ClientLayout::at(test_dir.path("client")),
    })
    .unwrap();

    let keys = TestKey::player(0);
    let mut client = App::new();
    client.add_plugins((OneThreadPlugins, TimePlugin, StatesPlugin));
    let setup = SimClientPlugin {
        main: Arc::new(TestKey::client_signer(keys.main)),
        session: SessionSigner::new(TestKey::client_signer(keys.session)),
        server: server.pin(),
        role: JoinRole::Play,
        hosting: Hosting::Local,
        packages,
        os: Os {
            clock: UnixClock::manual(TestKey::NOW),
            entropy: TestKey::client_entropy(),
        },
        data: None,
        bot: None,
    };
    // The lane runs at 30 Hz alone: a server pinned at 20 Hz fails the check, as a listing at a
    // rate the mode does not run at does.
    assert_eq!(setup.check(), Ok(()));
    let elsewhere = ServerPin {
        tick_hz: NonZeroU32::new(20).unwrap(),
        ..setup.server
    };
    let at_20 = SimClientPlugin {
        server: elsewhere,
        packages: Arc::clone(&setup.packages),
        main: Arc::clone(&setup.main),
        os: setup.os.clone(),
        data: None,
        session: SessionSigner::new(TestKey::client_signer(keys.session)),
        role: setup.role,
        hosting: setup.hosting,
        bot: None,
    };
    assert_eq!(at_20.check(), Err(TermsError::TickRate(elsewhere.tick_hz)));
    client.add_plugins(setup);
    client.finish();
    client.cleanup();
    let entity = SimClientPlugin::spawn(client.world_mut());
    client
        .world_mut()
        .entity_mut(entity)
        .insert(server.take_link());
    client.world_mut().trigger(Connect { entity });
    let start = Instant::now();
    let frame = |client: &mut App| {
        assert!(start.elapsed() < DEADLINE, "the client did not play on");
        client.update();
        thread::sleep(FRAME);
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

    // A report of a tick a fifth of a second past the one the client draws, which the server
    // runs later: the server writes its log so far into the report's folder of the client's data,
    // once the log ran the tick and its journal synced the records.
    let tick = (client.world().resource::<SimTick>().start()).after(Ticks::new(6));
    save(
        &mut client,
        SaveCommand::Report(ReportKey { tick, take: 0 }),
    );
    let reported = |test_dir: &TestDir| {
        if !test_dir.exists("client/reports") {
            return None;
        }
        let folder = test_dir.names("client/reports").pop()?;
        let file = format!("client/reports/{folder}/session.campfire-log");
        test_dir
            .exists(&file)
            .then(|| (folder, test_dir.read(file)))
    };
    let (folder, report) = loop {
        frame(&mut client);
        if let Some(reported) = reported(&test_dir) {
            break reported;
        }
    };

    // Dropped, the server ends the session, aborted as the mode did not end the match, and
    // publishes its log.
    drop(server);
    let stopped = ServerDir::open(&test_dir.path("data")).unwrap();
    assert!(SessionDir::find(&stopped).unwrap().is_none());
    let logs = test_dir.names("data/logs");
    assert_eq!(logs.len(), 1);
    let log = SessionLog::decode(&test_dir.read(format!("data/logs/{}", logs[0]))).unwrap();
    assert_eq!(
        log.result().map(|result| result.outcome),
        Some(Outcome::Aborted)
    );
    let saves: Vec<_> = log.checkpoints().collect();
    assert_eq!(saves.len(), 1);
    assert!(log.next_tick() > saves[0].tick);
    // The report's log is the session's so far: of its session, named with the tick reported,
    // through that tick at least, short of the published log's end, and of no result yet.
    let report = SessionLog::decode(&report).unwrap();
    assert_eq!(folder, format!("{}-{}", log.session_id(), tick.get()));
    assert_eq!(report.session_id(), log.session_id());
    assert!(report.next_tick() > tick);
    assert!(report.next_tick() <= log.next_tick());
    assert_eq!(report.checkpoints().count(), 1);
    assert_eq!(report.result(), None);
    // It replays to its end, its seed revealed, to the state the whole log's replay holds there.
    let packages = TestMode::Lane.packages();
    let end = report.next_tick();
    let mut replay = Replay::of(report, &packages).unwrap();
    replay.run().unwrap();
    let reported = replay.game().state_hash();
    let mut whole = Replay::of(log, &packages).unwrap();
    while whole.game().session().next_tick() < end {
        assert!(whole.run_tick().unwrap());
    }
    assert_eq!(whole.game().state_hash(), reported);
}
