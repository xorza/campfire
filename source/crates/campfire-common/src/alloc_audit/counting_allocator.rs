use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::{mem, thread};

use backtrace::Backtrace;

use crate::alloc_audit::alloc_trace::AllocTrace;

/// The allocator of a crate's `tests/alloc` binary: `System`, which counts each allocation and
/// reallocation of a thread inside a window that thread opened, and keeps its backtrace. Its
/// counts are each thread's own, as a test runner runs tests side by side in one process; so an
/// allocation on another thread is never counted, and a fixture whose work a thread of its own
/// does drives that work on its own thread to measure it.
#[derive(Debug)]
pub struct CountingAllocator;

/// What one window counted: its allocations and reallocations, their bytes, and the first
/// `CountingAllocator::TRACE_CAP` of their backtraces.
#[derive(Debug)]
pub struct AllocWindow {
    pub allocs: u64,
    pub bytes: u64,
    pub(crate) traces: Vec<AllocTrace>,
}

// A global allocator gets no state of its caller, so a window's state is the thread's own.
thread_local! {
    static IN_WINDOW: Cell<bool> = const { Cell::new(false) };
    static EXCLUDED: Cell<bool> = const { Cell::new(false) };
    /// Set while the allocator keeps a backtrace, whose own allocations it does not count.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
    static TRACES: RefCell<Vec<AllocTrace>> = const { RefCell::new(Vec::new()) };
}

/// Clears the thread's window as it closes, also when its body panics, so the thread's next window
/// starts closed.
#[derive(Debug)]
struct WindowGuard;

/// Gives the thread back the exclusion it had before an excluded part, as the part ends, also
/// when it panics.
#[derive(Debug)]
struct ExcludedGuard {
    was: bool,
}

impl CountingAllocator {
    /// The backtraces a window keeps; its counts stay exact past them. Each capture walks the
    /// stack and each one a failure prints resolves its symbols, which on Windows go through
    /// `dbghelp` behind one lock for the process, so the bound keeps a failure's cost flat.
    pub const TRACE_CAP: usize = 8;

    /// Runs `body` with the thread's allocations counted. A window inside an open one panics, as it
    /// would take the open window's backtraces and close it, and so does one inside an excluded
    /// part, which would count nothing.
    pub fn window(body: impl FnOnce()) -> AllocWindow {
        assert!(
            !IN_WINDOW.get(),
            "an allocation window opened inside an open one"
        );
        assert!(
            !EXCLUDED.get(),
            "an allocation window opened inside an excluded part"
        );
        IN_WINDOW.set(true);
        let guard = WindowGuard;
        TRACES.with_borrow_mut(Vec::clear);
        let allocs = ALLOCS.get();
        let bytes = BYTES.get();
        body();
        drop(guard);
        AllocWindow {
            allocs: ALLOCS.get() - allocs,
            bytes: BYTES.get() - bytes,
            traces: TRACES.with_borrow_mut(mem::take),
        }
    }

    /// Runs `body` with the thread's allocations not counted, inside a window or not: a fixture's
    /// own setup between the steps it measures, as a peer's side of a link.
    pub fn excluded<R>(body: impl FnOnce() -> R) -> R {
        let guard = ExcludedGuard {
            was: EXCLUDED.replace(true),
        };
        let result = body();
        drop(guard);
        result
    }

    fn track(size: usize) {
        // A panicking thread's allocations are its unwinding's, not the work a window measures.
        // Skipping them also keeps the allocator out of `dbghelp` under a panic hook that is in
        // it already, where two panicking threads deadlock on Windows.
        if !IN_WINDOW.get() || EXCLUDED.get() || CAPTURING.get() || thread::panicking() {
            return;
        }
        ALLOCS.set(ALLOCS.get() + 1);
        BYTES.set(BYTES.get() + size as u64);
        CAPTURING.set(true);
        TRACES.with_borrow_mut(|traces| {
            if traces.len() < CountingAllocator::TRACE_CAP {
                traces.push(AllocTrace::new(Backtrace::new_unresolved()));
            }
        });
        CAPTURING.set(false);
    }
}

#[expect(
    unsafe_code,
    reason = "a global allocator is an unsafe trait; each call goes to `System` unchanged"
)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CountingAllocator::track(layout.size());
        // SAFETY: the caller's contract for `alloc` is `System.alloc`'s.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` with `layout`, as every allocation here does.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        CountingAllocator::track(layout.size());
        // SAFETY: the caller's contract for `alloc_zeroed` is `System.alloc_zeroed`'s.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        CountingAllocator::track(new_size);
        // SAFETY: `ptr` came from `System` with `layout`, and the caller's contract for `realloc`
        // is `System.realloc`'s.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

impl AllocWindow {
    /// How many backtraces the window kept: its allocations up to `CountingAllocator::TRACE_CAP`.
    pub const fn traced(&self) -> usize {
        self.traces.len()
    }
}

impl Drop for WindowGuard {
    fn drop(&mut self) {
        IN_WINDOW.set(false);
    }
}

impl Drop for ExcludedGuard {
    fn drop(&mut self) {
        EXCLUDED.set(self.was);
    }
}
