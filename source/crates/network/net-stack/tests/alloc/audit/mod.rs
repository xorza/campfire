//! [`Audit`] is the one way in: a fixed warmup, then a run of measured frames, each held to the
//! fixture's budget.
//!
//! [`Audit::run`] is `#[track_caller]`, so a failure names the fixture. It runs every frame under
//! a dispatcher with no subscriber: a log line costs what the subscriber the application picks
//! makes it cost, so the audit counts the stack's own work and not the harness's spans and logs.

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "an audit reports its frames and a failure's backtraces to the terminal"
)]

mod format;

use std::panic::Location;

use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};
use tracing::subscriber::NoSubscriber;
use tracing::{Dispatch, dispatcher};

use crate::allocator;
use crate::allocator::AuditWindow;

/// Frames before the measured run. The last buffer to grow is bevy_replicon's mutation message,
/// when its ticks reach 128 and their postcard varints take a second byte; each frame sends one,
/// and the frames before the audit only add to the count. The next growth, at 16384, is far
/// past the measured run.
const WARMUP: usize = 128;

/// Frames measured after the warmup, each held to the budget.
const FRAMES: usize = 256;

/// One allocation audit: `Audit::new().run(frame)` allows each measured frame no allocation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Audit {
    budget: u64,
}

impl Audit {
    pub(crate) const fn new() -> Self {
        Self { budget: 0 }
    }

    /// What one measured frame may allocate. Zero unless the fixture says why.
    pub(crate) const fn budget(mut self, allocs: u64) -> Self {
        self.budget = allocs;
        self
    }

    /// Warms `frame`, then fails on the first measured frame over the budget.
    #[track_caller]
    pub(crate) fn run(self, frame: impl FnMut()) {
        let at = Location::caller();
        dispatcher::with_default(&Dispatch::new(NoSubscriber::default()), || {
            self.measure(at, frame);
        });
    }

    fn measure(self, at: &'static Location<'static>, mut frame: impl FnMut()) {
        assert_one_thread();
        (0..WARMUP).for_each(|_| frame());
        let mut worst = 0;
        let mut total = 0;
        for index in 0..FRAMES {
            let window = allocator::with_audit(&mut frame);
            if window.allocs > self.budget {
                self.fail(at, index, &window);
            }
            worst = worst.max(window.allocs);
            total += window.allocs;
        }
        println!(
            "alloc-audit {at}: worst {worst}, total {total}, budget {} a frame, over {FRAMES} \
             frames after {WARMUP} warmup",
            self.budget,
        );
    }

    fn fail(self, at: &'static Location<'static>, index: usize, window: &AuditWindow) -> ! {
        eprintln!(
            "alloc-audit {at}: frame {index}/{FRAMES} after {WARMUP} warmup allocated {} times, \
             {} B; the budget is {} a frame",
            window.allocs, window.bytes, self.budget,
        );
        for (i, trace) in window.traces.iter().enumerate() {
            eprintln!("--- allocation {i} ---\n{}", format::frames(trace));
        }
        let traced = window.traces.len() as u64;
        if window.allocs > traced {
            eprintln!(
                "({} more allocations not traced: a window keeps the first {traced})",
                window.allocs - traced,
            );
        }
        eprintln!("(set CAMPFIRE_ALLOC_FULL_BT=1 to see the whole stacks)");
        panic!(
            "alloc-audit {at}: frame {index} allocated {} times, over the budget of {}",
            window.allocs, self.budget,
        );
    }
}

/// The counter sees one thread, so the audit holds only while every task pool runs on the
/// calling thread, which it does while `bevy_tasks` builds without `multi_threaded`.
fn assert_one_thread() {
    let pools = [
        ComputeTaskPool::try_get().map(|pool| pool.thread_num()),
        AsyncComputeTaskPool::try_get().map(|pool| pool.thread_num()),
        IoTaskPool::try_get().map(|pool| pool.thread_num()),
    ];
    for threads in pools.into_iter().flatten() {
        assert_eq!(
            threads, 1,
            "a task pool runs work on other threads, which the counter misses"
        );
    }
}
