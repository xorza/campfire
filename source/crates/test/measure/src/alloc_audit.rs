//! The allocation audit: each crate's `tests/alloc` binary installs `CountingAllocator`, and each
//! of its fixtures drives one hot path a frame at a time through `AllocAudit`, which fails on the
//! first frame past its budget with that frame's backtraces.

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "an audit reports its frames, and a failing one each allocation's backtrace, to the terminal"
)]

use std::panic::Location;

use crate::counting_allocator::{AllocWindow, CountingAllocator};

/// One audit: the frames its fixture warms up for, past the last growth of its buffers, how many
/// frames it measures, and the allocations each of those may make, each named by what makes it
/// (11-net-stack/testing.md, An audit holds each frame to a budget after a fixed warmup).
/// `AllocAudit::new(warmup).run(frame)` measures 64 frames, none of them allocating.
#[derive(Debug, Clone, Copy)]
pub struct AllocAudit {
    warmup: usize,
    frames: usize,
    budget: &'static [&'static str],
}

/// What an audit measured: its worst frame, the slack a budget has, and the count most frames
/// read, the smaller on a tie.
#[derive(Debug, Clone, Copy)]
pub struct AuditReport {
    pub worst: u64,
    pub mode: u64,
}

impl AllocAudit {
    pub const fn new(warmup: usize) -> Self {
        Self {
            warmup,
            frames: 64,
            budget: &[],
        }
    }

    #[must_use]
    pub const fn frames(mut self, frames: usize) -> Self {
        self.frames = frames;
        self
    }

    /// The allocations one measured frame may make, each named by what makes it. A budget of
    /// some pins a flat cost, not an absent one: a cost that grew with the frames would pass it.
    #[must_use]
    pub const fn budget(mut self, allocs: &'static [&'static str]) -> Self {
        self.budget = allocs;
        self
    }

    const fn allowed(self) -> u64 {
        self.budget.len() as u64
    }

    /// Warms `frame` up, then measures each frame on its own, so a growth on the nth frame fails
    /// on that frame, with its backtraces. The caller names itself in the report and the failure.
    #[track_caller]
    pub fn run(self, mut frame: impl FnMut()) -> AuditReport {
        assert!(self.frames > 0, "an audit measures at least one frame");
        let at = Location::caller();
        for _ in 0..self.warmup {
            frame();
        }
        let mut counts = Vec::with_capacity(self.frames);
        for index in 0..self.frames {
            let window = CountingAllocator::window(&mut frame);
            if window.allocs > self.allowed() {
                self.fail(at, index, window);
            }
            counts.push(window.allocs);
        }
        counts.sort_unstable();
        let worst = *counts.last().expect("an audit measures at least one frame");
        let mode = counts
            .chunk_by(|a, b| a == b)
            .fold((0, 0), |(best, run), chunk| {
                if chunk.len() > run {
                    (chunk[0], chunk.len())
                } else {
                    (best, run)
                }
            })
            .0;
        println!(
            "alloc-audit {at}: worst {worst}, mode {mode}, budget {} over {} frames after {} of \
             warmup",
            self.allowed(),
            self.frames,
            self.warmup,
        );
        AuditReport { worst, mode }
    }

    fn fail(self, at: &'static Location<'static>, index: usize, mut window: AllocWindow) -> ! {
        eprintln!(
            "alloc-audit {at}: frame {index} of {} (after {} of warmup) allocated {} times, {} B; \
             the budget is {} a frame: {:?}",
            self.frames,
            self.warmup,
            window.allocs,
            window.bytes,
            self.allowed(),
            self.budget,
        );
        let traced = window.traces.len();
        for (number, trace) in window.traces.iter_mut().enumerate() {
            eprintln!("--- allocation {number} ---\n{}", trace.render());
        }
        if window.allocs > traced as u64 {
            eprintln!(
                "({} more allocations have no backtrace: a window keeps its first {traced})",
                window.allocs - traced as u64,
            );
        }
        eprintln!("(set CAMPFIRE_ALLOC_FULL_BT=1 to see each backtrace whole)");
        panic!(
            "alloc budget exceeded at {at} on frame {index}: {} allocations, budget {}",
            window.allocs,
            self.allowed(),
        );
    }
}
