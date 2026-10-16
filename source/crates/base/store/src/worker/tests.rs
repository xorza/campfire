use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;

use super::*;

#[test]
fn a_worker_passes_its_panic_on_when_dropped() {
    let worker = Worker::start("panics", || panic::panic_any(7_u32));
    let passed = panic::catch_unwind(AssertUnwindSafe(|| drop(worker))).unwrap_err();
    assert_eq!(passed.downcast_ref::<u32>(), Some(&7));
}

#[test]
fn a_check_passes_on_the_panic_of_a_worker_that_ended_and_does_nothing_while_it_runs() {
    // Running, the worker waits for the word to panic: a check passes nothing on.
    let (go, wait) = mpsc::channel::<()>();
    let mut worker = Worker::start("waits", move || {
        wait.recv().unwrap();
        panic::panic_any(9_u32);
    });
    worker.check();
    go.send(()).unwrap();
    while !worker.thread.as_ref().unwrap().is_finished() {
        thread::yield_now();
    }
    let passed = panic::catch_unwind(AssertUnwindSafe(|| worker.check())).unwrap_err();
    assert_eq!(passed.downcast_ref::<u32>(), Some(&9));
}
