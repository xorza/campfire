use std::num::{NonZeroU8, NonZeroU32};

use bevy_state::state::{NextState, State};
use campfire_common::ExitStatus;
use campfire_mode::OptionPicks;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    InputRules, Journal, SeatPlan, SeatStart, SeedChain, SessionHeader, SessionLog,
};
use campfire_runner::{HostChoices, SessionRules};
use campfire_store::AppendWriter;
use campfire_test_packages::TestMode;

use campfire_threads::ThreadName;

use super::*;
use crate::sim_server::session_journal::SessionJournal;

#[test]
fn a_failed_journal_ends_the_server_with_its_exit_code() {
    // A session of the test lane mode with one open seat, so its header needs no player; the
    // log's header is its journal's first record, whose sync fails.
    let packages = TestMode::Lane.packages();
    let key = TestKey::of(8);
    let rules = SessionRules::of(&packages);
    let terms = rules
        .terms(HostChoices {
            server_key: key.x_only_public_key().0,
            seed_commitment: SeedChain::new([9; 32], NonZeroU32::MIN).commitment(),
            tick_hz: packages.manifest().tick_hz.default(),
            inputs: InputRules::LAN,
            seats: vec![SeatPlan::Open],
            visibility: rules.visibility().settings(),
            picks: &OptionPicks::defaults(rules.options()),
        })
        .unwrap();
    let header = SessionHeader {
        terms,
        seats: vec![SeatStart::Open],
    };
    let log = SessionLog::new(header).unwrap();
    let journal = SessionJournal(AppendWriter::failing_sync(ThreadName::fixture("journal")));
    let mut world = World::new();
    world.init_resource::<Faults>();
    world.insert_resource(State::new(ServerPhase::Lobby));
    world.init_resource::<NextState<ServerPhase>>();
    world.insert_resource(JournalWatch(journal.watch()));
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    let journal = Journal::start(&log, Box::new(journal));
    // Closed, as a session that ends closes it, the log's journal waits for its writer, which
    // stopped at the failed sync, so the failure is there for the exit.
    drop(journal);
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(
        world.exit_due(StopRequest::NotAsked),
        Some(AppExit::Error(code))
    );
    // The failure is given once; before the match, only a stop exits.
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    assert_eq!(world.exit_due(StopRequest::Asked), Some(AppExit::Success));
}
