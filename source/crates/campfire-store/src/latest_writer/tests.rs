use std::sync::mpsc;
use std::thread;

use super::*;

#[test]
fn the_newest_value_replaces_one_not_written_and_each_failure_is_taken_once() {
    // The worker writes into `written`, after the test's word for each write; odd values fail.
    let (go, gate) = mpsc::channel::<()>();
    let written = Arc::new(Mutex::new(Vec::new()));
    let into = Arc::clone(&written);
    let writer = LatestWriter::start("latest", move |value: &u32| {
        gate.recv().unwrap();
        into.lock().unwrap().push(*value);
        if value % 2 == 1 { Err(*value) } else { Ok(()) }
    });
    // 1 is taken at once and waits for its word; 2 and 3 come while it waits, so 3 replaces 2.
    writer.give(1);
    while writer.shared.lock().newest.is_some() {
        thread::yield_now();
    }
    writer.give(2);
    writer.give(3);
    go.send(()).unwrap();
    go.send(()).unwrap();
    while writer.shared.lock().failures.len() < 2 {
        thread::yield_now();
    }
    assert_eq!(*written.lock().unwrap(), [1, 3]);
    assert_eq!(writer.take_failures(), [1, 3]);
    assert!(writer.take_failures().is_empty());
    // Dropped, the writer writes the value it holds.
    writer.give(4);
    go.send(()).unwrap();
    drop(writer);
    assert_eq!(*written.lock().unwrap(), [1, 3, 4]);
}
