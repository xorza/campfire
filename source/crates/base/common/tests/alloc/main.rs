//! The allocation audit's own checks, which every crate's `tests/alloc` suite relies on without
//! checking: exact counts, silence outside a window and in its excluded parts, no count from
//! another thread or from the audit's own bookkeeping, a thread left clean by a panicking body,
//! and a budget that fails on the frame past it.

use std::hint::{self, black_box};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use campfire_common::internals::{AllocAudit, CountingAllocator};
use campfire_store::Worker;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// One allocation of 8 bytes that the optimizer cannot remove.
fn one_alloc() {
    black_box(Box::new(black_box(0_u64)));
}

/// The message of a panic that `body` raises.
fn panic_of(body: impl FnOnce()) -> String {
    let payload = panic::catch_unwind(AssertUnwindSafe(body)).expect_err("the body panics");
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => (*payload.downcast::<&str>().expect("a panic's text")).to_owned(),
    }
}

#[test]
fn a_window_counts_what_it_allocates_and_nothing_outside_it_or_its_excluded_parts() {
    for _ in 0..32 {
        one_alloc();
    }
    let window = CountingAllocator::window(|| {
        for _ in 0..5 {
            one_alloc();
        }
        CountingAllocator::excluded(|| {
            for _ in 0..7 {
                one_alloc();
            }
            // An excluded part inside another one leaves the outer one excluded.
            CountingAllocator::excluded(one_alloc);
            one_alloc();
        });
        one_alloc();
    });
    for _ in 0..32 {
        one_alloc();
    }
    // 5 before the excluded part and 1 after it, 8 bytes each.
    assert_eq!(window.allocs, 6);
    assert_eq!(window.bytes, 6 * 8);
    assert_eq!(CountingAllocator::window(|| {}).allocs, 0);
}

#[test]
fn another_threads_allocations_are_not_counted() {
    // The worker starts before the window, as a start allocates on the caller; an atomic flag,
    // which never allocates, starts its 1,000 allocations inside it.
    let go = Arc::new(AtomicBool::new(false));
    let start = Arc::clone(&go);
    let worker = Worker::start("alloc-sibling", move || {
        while !start.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        for _ in 0..1_000 {
            one_alloc();
        }
    });
    let window = CountingAllocator::window(|| {
        go.store(true, Ordering::Release);
        drop(worker);
    });
    assert_eq!(window.allocs, 0);
}

#[test]
fn the_audits_own_bookkeeping_is_not_counted_and_backtraces_stop_at_their_cap() {
    // Below the cap the backtrace list allocates and grows; above it, only the count goes on.
    for allocs in [CountingAllocator::TRACE_CAP - 1, 64] {
        let window = CountingAllocator::window(|| {
            for _ in 0..allocs {
                one_alloc();
            }
        });
        assert_eq!(window.allocs, allocs as u64);
        assert_eq!(
            window.traced(),
            allocs.min(CountingAllocator::TRACE_CAP),
            "{allocs} allocations"
        );
    }
    // A window keeps none of the last one's backtraces.
    assert_eq!(CountingAllocator::window(|| {}).traced(), 0);
}

#[test]
fn a_panic_or_a_misplaced_window_leaves_the_thread_clean() {
    let panics: fn() = || panic!("the fixture panicked");
    let nested: fn() = || {
        CountingAllocator::window(|| {});
    };
    for (body, message) in [
        (panics, "the fixture panicked"),
        (nested, "an allocation window opened inside an open one"),
    ] {
        assert_eq!(panic_of(|| drop(CountingAllocator::window(body))), message);
        assert_eq!(CountingAllocator::window(one_alloc).allocs, 1);
    }
    assert_eq!(
        panic_of(|| CountingAllocator::excluded(nested)),
        "an allocation window opened inside an excluded part"
    );
    assert_eq!(CountingAllocator::window(one_alloc).allocs, 1);
}

#[test]
fn an_audit_reports_its_worst_frame_and_fails_on_the_first_past_its_budget() {
    // Frames allocate 0, 1, 2, 0, 1, 2, …; a fixed warmup of 3 keeps that order, so 64 frames
    // hold 22 zeros, 21 ones and 21 twos.
    let mut frame = 0_usize;
    let report = AllocAudit::new().warmup(3).frames(64).budget(2).run(|| {
        for _ in 0..frame % 3 {
            one_alloc();
        }
        frame += 1;
    });
    assert_eq!((report.worst, report.mode), (2, 0));

    // The probe stops at two frames in a row that keep the budget, or at 8 frames. Frames that
    // allocate `frame % 3` keep a budget of 1 at frames 0 and 1, so measured frame 0 is frame 2,
    // which allocates 2. Frames that allocate on even frames never keep a budget of 0 twice in a
    // row, so measured frame 0 is frame 8, which allocates 1.
    let by_thirds: fn(usize) -> usize = |frame| frame % 3;
    let on_evens: fn(usize) -> usize = |frame| usize::from(frame % 2 == 0);
    for (allocs, budget, failed, frames) in [
        (by_thirds, 1, "on frame 0: 2 allocations, budget 1", 3),
        (on_evens, 0, "on frame 0: 1 allocations, budget 0", 9),
    ] {
        let mut frame = 0_usize;
        let message = panic_of(|| {
            AllocAudit::new().budget(budget).run(|| {
                for _ in 0..allocs(frame) {
                    one_alloc();
                }
                frame += 1;
            });
        });
        assert!(message.ends_with(failed), "{message}");
        assert_eq!(frame, frames);
    }
}
