use std::sync::{Mutex, mpsc};
use std::thread;

use super::*;

#[test]
fn the_newest_value_replaces_one_not_written_and_the_newest_failure_is_taken_once() {
    // The worker writes into `written`, after the test's word for each write; odd values fail.
    let (go, gate) = mpsc::channel::<()>();
    let written = Arc::new(Mutex::new(Vec::new()));
    let into = Arc::clone(&written);
    let writer = LatestWriter::start(ThreadName::fixture("latest"), move |value: &u32| {
        gate.recv().unwrap();
        into.lock().unwrap().push(*value);
        if value % 2 == 1 { Err(*value) } else { Ok(()) }
    });
    // 1 is taken at once and waits for its word; 2 and 3 come while it waits, so 3 replaces 2.
    writer.give(1);
    while writer.shared.slot.lock().newest.is_some() {
        thread::yield_now();
    }
    writer.give(2);
    writer.give(3);
    go.send(()).unwrap();
    go.send(()).unwrap();
    while writer.shared.slot.lock().failure != Some(3) {
        thread::yield_now();
    }
    // Both failed, and 3's failure replaced 1's, as the newest: it is taken once.
    assert_eq!(*written.lock().unwrap(), [1, 3]);
    assert_eq!(writer.take_failure(), Some(3));
    assert_eq!(writer.take_failure(), None);
    // Closed, the writer writes the value it holds, and gives back its failure, which a failure
    // stops nothing before: 4 is written, then 5, which fails.
    writer.give(4);
    go.send(()).unwrap();
    while writer.shared.slot.lock().newest.is_some() {
        thread::yield_now();
    }
    writer.give(5);
    go.send(()).unwrap();
    assert_eq!(writer.stop(), Some(5));
    assert_eq!(*written.lock().unwrap(), [1, 3, 4, 5]);
}
