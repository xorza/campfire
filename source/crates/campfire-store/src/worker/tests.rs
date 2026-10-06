use std::panic::{self, AssertUnwindSafe};

use super::*;

#[test]
fn a_worker_passes_its_panic_on_when_dropped() {
    let worker = Worker::start("panics", || panic::panic_any(7_u32));
    let passed = panic::catch_unwind(AssertUnwindSafe(|| drop(worker))).unwrap_err();
    assert_eq!(passed.downcast_ref::<u32>(), Some(&7));
}
