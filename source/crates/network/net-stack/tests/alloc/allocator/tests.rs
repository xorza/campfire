use crate::allocator;
use crate::allocator::TRACE_CAP;
use campfire_store::internals::Race;
use std::hint::black_box;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};

fn one_alloc() {
    black_box(Box::new(black_box(0_u64)));
}

#[test]
fn counts_each_allocation_and_its_bytes_inside_the_window_only() {
    (0..32).for_each(|_| one_alloc());
    let window = allocator::with_audit(|| (0..5).for_each(|_| one_alloc()));
    (0..32).for_each(|_| one_alloc());
    assert_eq!(window.allocs, 5);
    // Five boxed `u64`s, 8 bytes each.
    assert_eq!(window.bytes, 5 * 8);
    let empty = allocator::with_audit(|| {});
    assert_eq!(empty.allocs, 0);
    assert!(empty.traces.is_empty());
}

#[test]
fn keeps_the_first_traces_and_counts_past_them() {
    let window = allocator::with_audit(|| (0..TRACE_CAP + 3).for_each(|_| one_alloc()));
    assert_eq!(window.allocs, TRACE_CAP as u64 + 3);
    assert_eq!(window.traces.len(), TRACE_CAP);
}

#[test]
fn another_threads_allocations_stay_out_of_the_window() {
    // Both threads start before the window, as a start allocates on the caller; a flag releases
    // the other's allocations inside the window, and another holds the window open until they end.
    let go = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    let windows = Race::run(2, |index| {
        if index == 0 {
            Some(allocator::with_audit(|| {
                go.store(true, Ordering::Release);
                while !done.load(Ordering::Acquire) {
                    core::hint::spin_loop();
                }
            }))
        } else {
            while !go.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }
            (0..1000).for_each(|_| one_alloc());
            done.store(true, Ordering::Release);
            None
        }
    });
    let [Some(window), None] = windows.as_slice() else {
        panic!("one window, from the first racer");
    };
    assert_eq!(window.allocs, 0);
}

#[test]
fn a_panic_inside_the_window_closes_it() {
    let unwound = panic::catch_unwind(AssertUnwindSafe(|| {
        allocator::with_audit(|| panic!("inside the window"));
    }));
    assert!(unwound.is_err());
    let window = allocator::with_audit(one_alloc);
    assert_eq!(window.allocs, 1);
}
