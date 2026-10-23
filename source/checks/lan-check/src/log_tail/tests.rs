use campfire_common::PlayerSlot;
use campfire_host::{InputLogged, MatchStarted};
use campfire_store::AppendWriter;
use campfire_store::internals::TestDir;
use campfire_threads::ThreadName;

use super::*;

fn started(player_slot: u8) -> String {
    format!(
        r#"{{"level":"INFO","target":"t","fields":{{"message":"the match started","player_slot":{player_slot},"start_tick":5}}}}"#
    )
}

#[test]
fn a_tail_reads_each_line_once_as_its_file_grows() {
    let test_dir = TestDir::new();
    let mut tail = LogTail::new(Process::Bot(0), test_dir.root());
    let mut seen = 0;
    // No file yet.
    assert_eq!(tail.read_past::<MatchStarted>(&mut seen).unwrap(), []);
    let path = Process::Bot(0).log_path(test_dir.root());
    let first = format!("{}\n{}", started(1), &started(2)[..20]);
    AppendWriter::create(ThreadName::fixture("tail"), &path, first.as_bytes())
        .unwrap()
        .close();
    // The line the process did not end waits for its end.
    let player_slot = |player_slot| MatchStarted {
        player_slot: PlayerSlot::new(player_slot),
        start_tick: 5,
    };
    assert_eq!(
        tail.read_past::<MatchStarted>(&mut seen).unwrap(),
        [player_slot(1)]
    );
    assert_eq!(tail.read_past::<MatchStarted>(&mut seen).unwrap(), []);
    let len = u64::try_from(first.len()).unwrap();
    let writer = AppendWriter::reopen(ThreadName::fixture("tail"), &path, len).unwrap();
    let more = format!("{}\n{}\n", &started(2)[20..], started(3));
    writer.append(1, |bytes| bytes.extend(more.as_bytes()));
    writer.close();
    // Only the lines after the last read, the one finished among them; a reader of its own
    // count reads the whole log, each line parsed once.
    assert_eq!(
        tail.read_past::<MatchStarted>(&mut seen).unwrap(),
        [player_slot(2), player_slot(3)]
    );
    assert!(tail.read_past::<InputLogged>(&mut seen).unwrap().is_empty());
    let mut from_start = 0;
    assert_eq!(
        tail.read_past::<MatchStarted>(&mut from_start).unwrap(),
        [player_slot(1), player_slot(2), player_slot(3)]
    );
    assert_eq!((seen, from_start), (3, 3));
    assert_eq!(tail.read().unwrap().lines().len(), 3);
}
