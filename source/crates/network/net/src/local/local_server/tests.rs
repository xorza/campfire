//! A local server on its own thread, which a client app in the same process plays through
//! in-process channels: it opens a session in its data directory, the client's player takes slot
//! 0 and plays, saves and loads the save, the server starting again and the client taking its new
//! link, and writes its log into a report; once the server drops, the session ends and its log is
//! published. The report then opens on a local server of the same data directory, which replays
//! it to its client watching.
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_common::{Tick, Ticks};
use campfire_host::ClientLayout;
use campfire_host::Hosting;
use campfire_host::JoinRole;
use campfire_host::JoinState;
use campfire_host::JournalCommand;
use campfire_host::Os;
use campfire_host::Pace;
use campfire_host::PaceSpeed;
use campfire_host::ReceivedTick;
use campfire_host::ReportKey;
use campfire_host::ServerDir;
use campfire_host::ServerPin;
use campfire_host::SessionDir;
use campfire_host::SessionSigner;
use campfire_host::UnixClock;
use campfire_net_connection::Connect;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Entropy, Outcome, SessionLog};
use campfire_runner::{Replay, SessionWorld, TermsError};
use campfire_sim::{MatchRate, SimTick, SimWorld};
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;
use campfire_values::TeamSet;

use crate::local::local_server::{
    LocalLobby, LocalReplay, LocalServer, LocalServerSetup, LocalSession,
};
use crate::local_pace::LocalPacePlugin;
use crate::sim_client::{PendingJournalCommands, SimClientPlugin};

/// How long the client gets to join and play.
const DEADLINE: Duration = Duration::from_secs(10);

/// The time between two frames of the client, which its test runs by hand.
const FRAME: Duration = Duration::from_millis(2);

#[test]
fn a_client_plays_a_local_server_which_publishes_the_log_as_it_drops() {
    let test_dir = TestDir::new();
    let packages = Arc::new(TestMode::Lane.packages());
    // The match plays at four times its rate, the server and the client following one pace, as
    // a local match's player may speed it, so the waits on the server's clock are short.
    let pace = Arc::new(Pace::default());
    pace.set_speed(PaceSpeed::Quadruple);
    let mut server = LocalServer::start(LocalServerSetup {
        packages: Arc::clone(&packages),
        data: test_dir.path("data"),
        session: LocalSession::New(LocalLobby {
            bots: Vec::new(),
            role: JoinRole::Play,
            options: Vec::new(),
            player_slot_picks: Vec::new(),
        }),
        pace: Arc::clone(&pace),
        os: Os {
            clock: UnixClock::manual(TestKey::NOW),
            entropy: Entropy::Fixed(7),
        },
        reports: ClientLayout::at(test_dir.path("client")),
        console: None,
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
    client.add_plugins((
        setup,
        LocalPacePlugin {
            pace,
            tick_rate: MatchRate::new(server.pin().tick_hz).rate(),
        },
    ));
    client.finish();
    client.cleanup();
    let entity = SimClientPlugin::spawn(client.world_mut());
    client
        .world_mut()
        .entity_mut(entity)
        .insert(server.take_link());
    client.world_mut().trigger(Connect { entity });
    let start = Instant::now();
    let frame = |client: &mut App, waits: &str| {
        let state = client.world().resource::<JoinState>();
        assert!(
            start.elapsed() < DEADLINE,
            "the deadline passed while the client waited for {waits}, its next tick {}: playing \
             {:?}, watching {:?}, rejoining {}, lost {:?}, refused {:?}",
            client.world().resource::<SimTick>().start(),
            state.playing_clock(),
            state.watching_clock(),
            state.is_rejoining(),
            state.loss(),
            state.refusal(),
        );
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
        frame(&mut client, "its play");
        played += usize::from(playing(&client));
    }
    // A playing client holds the tick its newest state stands before, behind the one it
    // predicts.
    let received = client.world().resource::<ReceivedTick>().get();
    let received = received.expect("a playing client received a state");
    assert!(received <= client.world().resource::<SimTick>().start());

    // A save, then a load of it: the server starts again on its data, and the client loses its
    // link, takes the new one, and plays on.
    let save = |client: &mut App, command| {
        client
            .world_mut()
            .resource_mut::<PendingJournalCommands>()
            .push(command);
    };
    // The save is due at the boundary after the server's next tick, which the server reaches on
    // its own clock, so the load waits until the save's snapshot is written; a load before it
    // finds no save and is refused.
    save(&mut client, JournalCommand::Save);
    let saved = |test_dir: &TestDir| {
        let sessions = test_dir.names("data/sessions");
        sessions.iter().any(|session| {
            let snapshots = format!("data/sessions/{session}/snapshots");
            test_dir.exists(&snapshots) && !test_dir.names(snapshots).is_empty()
        })
    };
    while !saved(&test_dir) {
        frame(&mut client, "the save's snapshot");
    }
    save(&mut client, JournalCommand::LoadLatest);
    while playing(&client) {
        frame(&mut client, "the load");
    }
    let mut played = 0;
    while played < 30 {
        frame(&mut client, "its play after the load");
        played += usize::from(playing(&client));
    }

    // A report of a tick six ticks past the one the client draws, which the server
    // runs later: the server writes its log so far into the report's folder of the client's data,
    // once the log ran the tick and its journal synced the records.
    let tick = (client.world().resource::<SimTick>().start()).after(Ticks::new(6));
    save(
        &mut client,
        JournalCommand::Report(ReportKey { tick, take: 0 }),
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
        frame(&mut client, "the report");
        if let Some(reported) = reported(&test_dir) {
            break reported;
        }
    };

    // Dropped, the server ends the session, aborted as the mode did not end the match, and
    // publishes its log.
    drop(server);
    let stopped = ServerDir::open(&test_dir.path("data")).unwrap();
    assert!(SessionDir::find(&stopped).unwrap().is_none());
    drop(stopped);
    let logs = test_dir.names("data/logs");
    assert_eq!(logs.len(), 1);
    let log = SessionLog::decode(&test_dir.read(format!("data/logs/{}", logs[0]))).unwrap();
    let session = log.session_id();
    assert_eq!(
        log.result().map(|result| result.outcome),
        Some(Outcome::Aborted)
    );
    let saves: Vec<_> = log.checkpoints().collect();
    assert_eq!(saves.len(), 1);
    assert!(log.next_tick() > saves[0].tick);
    // The report's log is the session's so far: of its session, named with the tick reported,
    // through that tick at least, short of the published log's end, and of no result yet.
    let report_bytes = report;
    let report = SessionLog::decode(&report_bytes).unwrap();
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

    // A report of a tick five before the log's end opens on a local server of the same data
    // directory, whose key signed its log: it replays the log through that tick and holds
    // paused, its client watching from the tick after; played on, it runs the log's last five
    // ticks and holds paused again at its end.
    let pace = Arc::new(Pace::default());
    pace.set_speed(PaceSpeed::Quadruple);
    let mut server = LocalServer::start(LocalServerSetup {
        packages: Arc::new(TestMode::Lane.packages()),
        data: test_dir.path("data"),
        session: LocalSession::Replay(Box::new(LocalReplay {
            log: SessionLog::decode(&report_bytes).unwrap(),
            through: Tick::new(end.get() - 5),
        })),
        pace: Arc::clone(&pace),
        os: Os {
            clock: UnixClock::manual(TestKey::NOW),
            entropy: Entropy::Fixed(7),
        },
        reports: ClientLayout::at(test_dir.path("client")),
        console: None,
    })
    .unwrap();
    assert!(pace.is_paused());
    let mut watcher = App::new();
    watcher.add_plugins((OneThreadPlugins, TimePlugin, StatesPlugin));
    watcher.add_plugins(SimClientPlugin {
        main: Arc::new(TestKey::client_signer(keys.main)),
        session: SessionSigner::new(TestKey::client_signer(keys.session)),
        server: server.pin(),
        role: JoinRole::Watch {
            teams: TeamSet::ALL,
        },
        hosting: Hosting::Local,
        packages: Arc::new(TestMode::Lane.packages()),
        os: Os {
            clock: UnixClock::manual(TestKey::NOW),
            entropy: TestKey::client_entropy(),
        },
        data: None,
        bot: None,
    });
    watcher.add_plugins(LocalPacePlugin {
        pace: Arc::clone(&pace),
        tick_rate: MatchRate::new(server.pin().tick_hz).rate(),
    });
    watcher.finish();
    watcher.cleanup();
    let entity = SimClientPlugin::spawn(watcher.world_mut());
    watcher
        .world_mut()
        .entity_mut(entity)
        .insert(server.take_link());
    watcher.world_mut().trigger(Connect { entity });
    let clock = loop {
        frame(&mut watcher, "the watch");
        if let Some(clock) = watcher.world().resource::<JoinState>().watching_clock() {
            break clock;
        }
    };
    assert_eq!(clock.first(), Tick::new(end.get() - 4));
    // The watcher's tick stands at the state it holds, which reaches it while the replay holds
    // paused: before the tick after the one the replay holds at, then, played on, before the
    // log's end.
    let next = |watcher: &App| watcher.world().resource::<SimTick>().start();
    while next(&watcher) != clock.first() {
        frame(&mut watcher, "the held tick");
    }
    assert_eq!(
        watcher.world().resource::<ReceivedTick>().get(),
        Some(clock.first())
    );
    assert!(pace.is_paused());
    // A report the watcher takes there holds the log the replay replays, whole.
    let held = Tick::new(end.get() - 5);
    let again = ReportKey {
        tick: held,
        take: 0,
    };
    save(&mut watcher, JournalCommand::Report(again));
    let file = format!(
        "client/reports/{session}-{}/session.campfire-log",
        held.get()
    );
    while !test_dir.exists(&file) {
        frame(&mut watcher, "the report of the held tick");
    }
    assert_eq!(test_dir.read(file), report_bytes);
    // The replay holds at the log's end, where the watcher stands, never past it, as the server
    // runs no step past the last tick; a play there runs none either, for 50 ms, six of the
    // lane's ticks at the pace, past the server's next frame and a send.
    pace.set_paused(false);
    while !pace.is_paused() || next(&watcher) != end {
        frame(&mut watcher, "the replay's end");
        assert!(
            next(&watcher) <= end,
            "the watcher stands past the log's end"
        );
    }
    pace.set_paused(false);
    let played = Instant::now();
    while played.elapsed() < Duration::from_millis(50) {
        frame(&mut watcher, "a play at the end");
        assert!(next(&watcher) <= end, "a play at the end runs a step");
    }
    assert!(pace.is_paused());
    assert_eq!(next(&watcher), end);
    drop(server);
}
