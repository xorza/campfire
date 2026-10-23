//! A server that stops mid-match and starts again on its data directory: within the restore
//! window it replays its log and plays on; past it, it ends the session aborted and publishes
//! its log; it cuts a torn journal at its first bad frame; and it refuses a journal of another
//! session than its directory's, and a seed chain the session's terms do not commit to.

use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::{StateHash, Tick};
use campfire_harness::HashTrail;
use campfire_host::internals::TickHashes;
use campfire_host::{JournalCut, Publisher, RestoreError, ServerDir, SessionDir};
use campfire_log::internals::LogCheck;
use campfire_net_emulator::LinkModel;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Entropy, Outcome, SeedChain, SessionLog, SessionPrivate, Signer};
use campfire_runner::{Replay, Session};
use campfire_store::DurableDir;
use campfire_store::internals::TestDir;

/// The scenario's match, its server's data in `scratch`, after 90 steps; with the state hash after
/// each tick the server ran.
fn played(test_dir: &TestDir) -> (InProcessMatch, Vec<StateHash>) {
    let mut local = InProcessMatch::new(InProcessSetup::duo(
        LinkModel::perfect(),
        InProcessMatch::SEED_CHAIN,
    ));
    local.keep_data(test_dir.path("data"));
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
    let test_dir = TestDir::new();
    let (mut local, before) = played(&test_dir);
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
fn a_session_past_its_window_ends_aborted_its_journal_cut_at_a_tear() {
    let test_dir = TestDir::new();
    let (mut local, before) = played(&test_dir);
    let cut = local.next_tick(End::Server);
    let id = local
        .server()
        .world()
        .resource::<Session>()
        .log()
        .session_id();
    local.stop_server();

    // Under the name of another session, the directory's journal is refused, naming its own.
    let stopped = ServerDir::open(&test_dir.path("data")).unwrap();
    let session_dir = test_dir.path("data/sessions").join(id.to_string());
    let other = test_dir.path("data/sessions").join("0".repeat(64));
    DurableDir::rename(&session_dir, &other).unwrap();
    let refused = SessionDir::find(&stopped).unwrap().unwrap().restore().err();
    assert!(
        matches!(&refused, Some(RestoreError::OtherSession(session)) if *session == id),
        "{refused:?}"
    );
    DurableDir::rename(&other, &session_dir).unwrap();

    // The private record, at its path in the session's directory, holds another seed chain than
    // the one the journal's terms commit to: the restore refuses it.
    let path = session_dir.join("private");
    let ours = test_dir.read(&path);
    let mut bytes = Vec::new();
    let seed_chain = SeedChain::new([7; 32], NonZeroU32::MIN);
    SessionPrivate { seed_chain }.encode(&mut bytes);
    test_dir.write(&path, bytes);
    let dir = SessionDir::find(&stopped).unwrap().unwrap();
    let refused = dir.restore().err();
    assert!(
        matches!(&refused, Some(RestoreError::OtherSeedChain)),
        "{refused:?}"
    );
    test_dir.write(&path, ours);

    // A journal whose tail is no whole frame, as a crash or a bad disk leaves one, is cut there:
    // the restore logs the 5 bytes it drops, and the journal holds its whole frames alone.
    let journal = session_dir.join("journal");
    let whole = test_dir.read(&journal);
    test_dir.write(&journal, [whole.as_slice(), &[0xAB; 5]].concat());
    let log = LogCheck::start();
    let session = dir.restore().unwrap().unwrap();
    assert_eq!(
        log.take::<JournalCut>(),
        [JournalCut {
            session: id,
            bytes: 5,
        }]
    );
    assert_eq!(session.journal_len, u64::try_from(whole.len()).unwrap());
    assert_eq!(test_dir.read(&journal), whole);

    // Past the window, the session ends aborted before the tick it stopped at, in the state the
    // server left, and its log, published, replays to every hash.
    let key = TestKey::server();
    let file = session
        .abort(Publisher {
            data: &stopped,
            packages: local.packages(),
            signer: &Arc::new(Signer::new(key, Entropy::Fixed(6))),
        })
        .unwrap();
    assert!(SessionDir::find(&stopped).unwrap().is_none());
    let log = SessionLog::decode(&test_dir.read(&file)).unwrap();
    let result = *log.result().unwrap();
    assert_eq!(
        (result.tick, result.outcome, result.state_hash),
        (Tick::new(cut), Outcome::Aborted, *before.last().unwrap())
    );
    let mut replay = Replay::of(log, local.packages()).unwrap();
    let replayed = HashTrail::of_replay(&mut replay);
    HashTrail::of_totals(before).assert_same(&replayed.unwrap());
}
