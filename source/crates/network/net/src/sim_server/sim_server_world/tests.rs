use std::io;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_common::ExitStatus;
use campfire_package_disk::internals::TestMode;
use campfire_protocol::InputRules;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{SeedChain, SessionHeader, SessionLog, SlotPlan, SlotStart};
use campfire_runner::SessionRules;
use campfire_store::{AppendFile, AppendWriter};

use super::*;
use crate::sim_server::session_journal::SessionJournal;

/// A journal's file whose every sync fails.
#[derive(Debug)]
struct FailingFile;

impl AppendFile for FailingFile {
    fn append(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        Err(io::Error::other("sync failed"))
    }
}

#[test]
fn a_failed_journal_ends_the_server_with_its_exit_code() {
    // A session of the test lane mode with one open slot, so its header needs no player; the
    // log's header is its journal's first record, whose sync fails.
    let packages = TestMode::Lane.packages();
    let key = TestKey::of(8);
    let rules = SessionRules::of(&packages);
    let terms = rules
        .terms(
            key.x_only_public_key().0,
            SeedChain::new([9; 32], NonZeroU32::MIN).commitment(),
            packages.manifest().tick_hz.default(),
            InputRules::LAN,
            [SlotPlan::Open],
            rules.visibility().settings(),
        )
        .unwrap();
    let header = SessionHeader {
        terms,
        slots: vec![SlotStart::Open],
    };
    let mut log = SessionLog::new(header).unwrap();
    let journal = SessionJournal(AppendWriter::start("journal", FailingFile));
    let mut world = World::new();
    world.init_resource::<Faults>();
    world.insert_resource(JournalWatch(journal.watch()));
    assert_eq!(world.exit_due(false), None);
    log.keep_journal(Box::new(journal));
    // Dropped, the log's journal waits for its writer, which stopped at the failed sync.
    drop(log);
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(world.exit_due(false), Some(AppExit::Error(code)));
    // The failure is given once; before the match, only a stop exits.
    assert_eq!(world.exit_due(false), None);
    assert_eq!(world.exit_due(true), Some(AppExit::Success));
}
