//! A counting allocator over `System`. While a thread is inside [`with_audit`], each `alloc`,
//! `alloc_zeroed` and `realloc` bumps that thread's counters and keeps an unresolved backtrace;
//! `dealloc` is passed through, as the audit counts heap operations, not residency.
//!
//! The counters are per-thread, so audits run in parallel with no lock. The blind spot is work on
//! another thread, which the audit rules out by asserting one task-pool thread.

#![expect(
    clippy::disallowed_types,
    reason = "a global allocator has no owner to carry its counters"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::backtrace::Backtrace;
use std::cell::{Cell, RefCell};
use std::mem;
use std::thread;

#[derive(Debug)]
pub(crate) struct CountingAllocator;

thread_local! {
    static IN_AUDIT: Cell<bool> = const { Cell::new(false) };
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
    static TRACES: RefCell<Vec<Backtrace>> = const { RefCell::new(Vec::new()) };
}

/// The backtraces one window keeps. It bounds the diagnostic only: the counters stay exact, and
/// each capture walks the stack.
pub(crate) const TRACE_CAP: usize = 8;

fn track(size: usize) {
    // A panicking thread is unwinding, and its hook's own allocations are no frame's work.
    if !IN_AUDIT.get() || CAPTURING.get() || thread::panicking() {
        return;
    }
    ALLOCS.set(ALLOCS.get() + 1);
    BYTES.set(BYTES.get() + size as u64);
    CAPTURING.set(true);
    if TRACES.with_borrow(Vec::len) < TRACE_CAP {
        let trace = Backtrace::force_capture();
        TRACES.with_borrow_mut(|traces| traces.push(trace));
    }
    CAPTURING.set(false);
}

// SAFETY: every call forwards to `System` with the caller's arguments unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        track(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        track(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        track(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// What one window allocated on its thread.
#[derive(Debug)]
pub(crate) struct AuditWindow {
    pub(crate) allocs: u64,
    pub(crate) bytes: u64,
    /// The first [`TRACE_CAP`] allocations' backtraces; `allocs` counts past the cap.
    pub(crate) traces: Vec<Backtrace>,
}

/// Clears `IN_AUDIT` on drop, so a panic inside a window cannot leave the thread counting.
#[derive(Debug)]
struct AuditGuard;

impl AuditGuard {
    fn enter() -> Self {
        assert!(!IN_AUDIT.get(), "with_audit called inside an open window");
        IN_AUDIT.set(true);
        Self
    }
}

impl Drop for AuditGuard {
    fn drop(&mut self) {
        IN_AUDIT.set(false);
    }
}

/// Runs `f` with this thread's allocations counted.
pub(crate) fn with_audit(f: impl FnOnce()) -> AuditWindow {
    TRACES.with_borrow_mut(Vec::clear);
    let allocs = ALLOCS.get();
    let bytes = BYTES.get();
    let guard = AuditGuard::enter();
    f();
    drop(guard);
    AuditWindow {
        allocs: ALLOCS.get() - allocs,
        bytes: BYTES.get() - bytes,
        traces: TRACES.with_borrow_mut(mem::take),
    }
}

mod tests;
