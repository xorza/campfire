//! A server that stops mid-match and starts again on its data directory: within the restore
//! window it replays its log and plays on; past it, it ends the session aborted and publishes
//! its log; and it refuses a session of another release, naming it.

use campfire_common::{StateHash, Tick};
use campfire_net::internals::{End, InProcessMatch, LinkModel, MatchSetup};
use campfire_net::{RestoreError, ServerDir, SessionDir, TickHashes};
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Outcome, SessionLog, SessionPrivate};
use campfire_runner::{Runner, Session};
use campfire_store::Scratch;

/// The scenario's match, its server's data in `data`, after 90 steps; with the state hash after
/// each tick the server ran.
fn played(data: &Scratch) -> (InProcessMatch, Vec<StateHash>) {
    let mut local = InProcessMatch::new(MatchSetup::duo(
        LinkModel::PERFECT,
        InProcessMatch::SEED_CHAIN,
    ));
    local.keep_data(data.path("data"));
    local.start_match();
    local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
    for _ in 0..90 {
        local.step();
    }
    let hashes = local
        .server()
        .world()
        .resource::<TickHashes>()
        .get()
        .to_vec();
    (local, hashes)
}

#[test]
fn a_restarted_server_replays_its_log_and_plays_on() {
    let data = Scratch::new();
    let (mut local, before) = played(&data);
    let cut = local.next_tick(End::Server);
    assert_eq!(before.len(), usize::try_from(cut).unwrap());
    local.restart_server();
    // The replay reaches the same state after every tick, and the server logs both players'
    // slots disconnected before the tick it runs on from.
    let world = local.server().world();
    assert_eq!(world.resource::<TickHashes>().get(), before);
    assert_eq!(local.next_tick(End::Server), cut);
    let place = world.resource::<Session>().log().next_place();
    assert_eq!((place.tick, place.index), (Tick::new(cut), 2));
    // A step is a tick of the server, with no client.
    for _ in 0..30 {
        local.step();
    }
    assert_eq!(local.next_tick(End::Server), cut + 30);
}

#[test]
fn a_session_past_its_window_ends_aborted_and_one_of_another_release_is_refused() {
    let data = Scratch::new();
    let (mut local, before) = played(&data);
    let cut = local.next_tick(End::Server);
    let id = local
        .server()
        .world()
        .resource::<Session>()
        .log()
        .session_id();
    local.stop_server();

    // The private record, at the path Stage 6 names, names another release: the restore refuses
    // it, naming the release.
    let stopped = ServerDir::open(&data.path("data")).unwrap();
    let path = data
        .path("data/sessions")
        .join(id.to_string())
        .join("private");
    let ours = data.read(&path);
    let mut private = SessionPrivate::decode(&ours).unwrap();
    private.terms.release = "0.0.9".to_owned();
    let mut bytes = Vec::new();
    private.encode(&mut bytes);
    data.write(&path, bytes);
    let dir = SessionDir::find(&stopped).unwrap().unwrap();
    let refused = dir.restore().err();
    assert!(
        matches!(&refused, Some(RestoreError::OtherRelease(release)) if release == "0.0.9"),
        "{refused:?}"
    );

    // Its own release: past the window, the session ends aborted before the tick it stopped
    // at, in the state the server left, and its log, published, replays to every hash.
    data.write(&path, ours);
    let session = dir.restore().unwrap().unwrap();
    let key = TestKey::server();
    let file = session
        .abort(&stopped, local.packages(), key, |aux| aux.fill(6))
        .unwrap();
    assert!(SessionDir::find(&stopped).unwrap().is_none());
    let log = SessionLog::decode(&data.read(&file)).unwrap();
    let result = *log.result().unwrap();
    assert_eq!(
        (result.tick, result.outcome, result.state_hash),
        (Tick::new(cut), Outcome::Aborted, *before.last().unwrap())
    );
    let seeds = log.revealed_seeds().unwrap();
    let mut replay = Runner::new(log.rewound(), seeds, local.packages()).unwrap();
    let mut replayed = Vec::new();
    while replay.log().next_tick() < Tick::new(cut) {
        replay.run_tick();
        replayed.push(replay.state_hash());
    }
    assert_eq!(replayed, before);
    assert_eq!(replay.check_result(), Ok(()));
}
