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
