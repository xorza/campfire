use std::thread;
use std::time::Duration;

use super::*;
use crate::journal::journal_frames::JournalFrames;
use crate::journal::tests::MemoryFile;

/// The records of the journal `file` holds.
fn records(file: &[u8]) -> Vec<&[u8]> {
    JournalFrames::new(file).unwrap().collect()
}

#[test]
fn a_journal_rebuilds_the_log_it_followed() {
    // Kept from the start: each record goes to the journal as the log takes it.
    let mut log = new_log();
    let file = MemoryFile::new();
    log.keep_journal(Journal::start(file.clone()));
    play_minimal(&mut log);
    let bytes = encoded(&log);
    drop(log);
    let journal = file.bytes();
    let rebuilt = SessionLog::from_journal(records(&journal)).unwrap();
    assert_eq!(encoded(&rebuilt), bytes);
    // The header, a packet and a server input, the seal, the checkpoint, a packet, the result.
    let kinds: Vec<u8> = records(&journal).iter().map(|record| record[0]).collect();
    assert_eq!(kinds, [0, 1, 2, 3, 4, 1, 5]);

    // Kept at the end: the log writes what it holds, the same records.
    let mut log = new_log();
    play_minimal(&mut log);
    let late = MemoryFile::new();
    log.keep_journal(Journal::start(late.clone()));
    drop(log);
    assert_eq!(late.bytes(), journal);

    // Every prefix of the records rebuilds a log, as a crash leaves one: what the server held
    // when it wrote the last of them.
    let all = records(&journal);
    for len in 1..all.len() {
        assert!(
            SessionLog::from_journal(all[..len].iter().copied()).is_ok(),
            "{len}"
        );
    }

    // A journal with no header, or a record past it that the log refuses: the packet again,
    // whose first stamp, 0, goes back from its last, 1.
    assert_eq!(
        SessionLog::from_journal(all[1..].iter().copied()).err(),
        Some(JournalReplayError::NoHeader)
    );
    let again = [all[0], all[1], all[1]];
    assert_eq!(
        SessionLog::from_journal(again).err(),
        Some(JournalReplayError::Record {
            record: 2,
            error: LogError::Input {
                tick: Tick::new(0),
                error: InputError::StampBack,
            },
        })
    );
}

#[test]
fn the_log_follows_where_each_chain_stands_in_the_synced_records() {
    let mut log = new_log();
    let file = MemoryFile::new();
    let journal = Journal::start(file.clone());
    let watch = journal.watch();
    log.keep_journal(journal);
    play_minimal(&mut log);
    // No chain is durable before the log notes the synced records; once the writer synced all 7,
    // player 1's is.
    let slot = PlayerSlot::new(1);
    assert_eq!(log.durable_head(slot), None);
    for _ in 0..1000 {
        if watch.durable() == 7 {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(watch.durable(), 7);
    log.advance_durable();
    // Player 1's chain holds a, b and c: c, seq 2, the last, signed under their delegation.
    let mut chain = InputChain::new(slot, root(1));
    for (stamp, payload) in [(0, b"a"), (1, b"b"), (1, b"c")] {
        chain.extend(Tick::new(stamp), payload);
    }
    let head = DurableHead {
        delegation: *delegation(1).id(),
        seq: 2,
        head: chain.head(),
    };
    assert_eq!(log.durable_head(slot), Some(head));
    assert_eq!(log.durable_head(PlayerSlot::new(0)), None);

    // The log the journal rebuilds, once it keeps the journal again, holds every chain durably.
    drop(log);
    let journal = file.bytes();
    let mut rebuilt = SessionLog::from_journal(records(&journal)).unwrap();
    assert_eq!(rebuilt.durable_head(slot), None);
    rebuilt.resume_journal(Journal::start(MemoryFile::new()));
    assert_eq!(rebuilt.durable_head(slot), Some(head));
    assert_eq!(rebuilt.durable_head(PlayerSlot::new(0)), None);
}
