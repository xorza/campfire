#![expect(
    clippy::disallowed_types,
    reason = "a global allocator gets no state of its caller, so a thread's exclusion is its own"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::{mem, thread};

use backtrace::Backtrace;

use crate::alloc_audit::alloc_trace::AllocTrace;

/// The allocator of a crate's `tests/alloc` binary: `System`, which counts each allocation and
/// reallocation of every thread of the process inside an open window, and keeps its backtrace,
/// so a fixture's workers count as its own thread does. A window is the process's, as nextest
/// runs each test in a process of its own; a thread's excluded part is that thread's alone.
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

/// The process's window and what it counted: statics, as a global allocator gets no state of
/// its caller.
static IN_WINDOW: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static TRACES: Mutex<Vec<AllocTrace>> = Mutex::new(Vec::new());

thread_local! {
    static EXCLUDED: Cell<bool> = const { Cell::new(false) };
    /// Set while the thread keeps a backtrace, or holds the traces' lock, whose own allocations
    /// the allocator does not count.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
}

/// Closes the process's window, also when its body panics, so the next window starts closed.
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

    /// Runs `body` with the process's allocations counted. A window inside an open one panics, as
    /// it would take the open window's backtraces and close it, and so does one inside an
    /// excluded part, whose thread would count nothing.
    pub fn window(body: impl FnOnce()) -> AllocWindow {
        assert!(
            !IN_WINDOW.load(Ordering::Acquire),
            "an allocation window opened inside an open one"
        );
        assert!(
            !EXCLUDED.get(),
            "an allocation window opened inside an excluded part"
        );
        Self::traces(Vec::clear);
        let allocs = ALLOCS.load(Ordering::Acquire);
        let bytes = BYTES.load(Ordering::Acquire);
        IN_WINDOW.store(true, Ordering::Release);
        let guard = WindowGuard;
        body();
        drop(guard);
        AllocWindow {
            allocs: ALLOCS.load(Ordering::Acquire) - allocs,
            bytes: BYTES.load(Ordering::Acquire) - bytes,
            traces: Self::traces(mem::take),
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
        if !IN_WINDOW.load(Ordering::Acquire)
            || EXCLUDED.get()
            || CAPTURING.get()
            || thread::panicking()
        {
            return;
        }
        ALLOCS.fetch_add(1, Ordering::AcqRel);
        BYTES.fetch_add(size as u64, Ordering::AcqRel);
        Self::traces(|traces| {
            if traces.len() < Self::TRACE_CAP {
                traces.push(AllocTrace::new(Backtrace::new_unresolved()));
            }
        });
    }

    /// `f` of the window's traces, with the thread's allocations as it holds them not counted,
    /// so a lock that allocates as it first locks, or a trace's own buffer, counts nothing.
    fn traces<R>(f: impl FnOnce(&mut Vec<AllocTrace>) -> R) -> R {
        let was = CAPTURING.replace(true);
        let result = f(&mut TRACES.lock().unwrap_or_else(PoisonError::into_inner));
        CAPTURING.set(was);
        result
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::track(layout.size());
        // SAFETY: the caller's contract for `alloc` is `System.alloc`'s.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` with `layout`, as every allocation here does.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::track(layout.size());
        // SAFETY: the caller's contract for `alloc_zeroed` is `System.alloc_zeroed`'s.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::track(new_size);
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
        IN_WINDOW.store(false, Ordering::Release);
    }
}

impl Drop for ExcludedGuard {
    fn drop(&mut self) {
        EXCLUDED.set(self.was);
    }
}
