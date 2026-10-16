use std::num::{NonZeroU8, NonZeroU32};

use campfire_common::ExitStatus;
use campfire_package_disk::internals::TestMode;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{InputRules, SeedChain, SessionHeader, SessionLog, SlotPlan, SlotStart};
use campfire_runner::{HostChoices, SessionRules};
use campfire_store::AppendWriter;

use super::*;
use crate::sim_server::session_journal::SessionJournal;

#[test]
fn a_failed_journal_ends_the_server_with_its_exit_code() {
    // A session of the test lane mode with one open slot, so its header needs no player; the
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
            slots: vec![SlotPlan::Open],
            visibility: rules.visibility().settings(),
        })
        .unwrap();
    let header = SessionHeader {
        terms,
        slots: vec![SlotStart::Open],
    };
    let mut log = SessionLog::new(header).unwrap();
    let journal = SessionJournal(AppendWriter::failing_sync("journal"));
    let mut world = World::new();
    world.init_resource::<Faults>();
    world.insert_resource(JournalWatch(journal.watch()));
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    log.keep_journal(Box::new(journal));
    // Closed, as a session that ends closes it, the log's journal waits for its writer, which
    // stopped at the failed sync, so the failure is there for the exit.
    log.close_journal();
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(
        world.exit_due(StopRequest::NotAsked),
        Some(AppExit::Error(code))
    );
    // The failure is given once; before the match, only a stop exits.
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    assert_eq!(world.exit_due(StopRequest::Asked), Some(AppExit::Success));
}
