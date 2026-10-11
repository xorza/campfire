//! The allocation audit: each crate's `tests/alloc` binary installs `CountingAllocator`, and each
//! of its fixtures drives one hot path a frame at a time through `AllocAudit`, which fails on the
//! first frame past its budget with that frame's backtraces.

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "an audit reports its frames, and a failing one each allocation's backtrace, to the terminal"
)]

mod alloc_trace;
pub(crate) mod counting_allocator;

use std::panic::Location;

use crate::alloc_audit::counting_allocator::{AllocWindow, CountingAllocator};

/// One audit: how its fixture warms up, how many frames it measures, and what each of those may
/// allocate. `AllocAudit::new().run(frame)` is the whole call for most fixtures: the probe, 64
/// frames, none of them allocating.
#[derive(Debug, Clone, Copy)]
pub struct AllocAudit {
    warmup: Warmup,
    frames: usize,
    budget: u64,
}

/// How the warmup ends.
#[derive(Debug, Clone, Copy)]
enum Warmup {
    /// Once `STABLE_RUN` frames in a row keep the budget, or after `MAX_WARMUP`. Wrong for a
    /// fixture that cycles, whose quiet frames may come before its widest one: that one's growth
    /// then lands in the measured frames, and such a fixture warms up for whole cycles.
    Probe,
    Fixed(usize),
}

/// What an audit measured: its worst frame, the slack a budget has, and the count most frames
/// read, the smaller on a tie.
#[derive(Debug, Clone, Copy)]
pub struct AuditReport {
    pub worst: u64,
    pub mode: u64,
}

impl AllocAudit {
    const MAX_WARMUP: usize = 8;
    const STABLE_RUN: usize = 2;

    pub const fn new() -> AllocAudit {
        AllocAudit {
            warmup: Warmup::Probe,
            frames: 64,
            budget: 0,
        }
    }

    /// A warmup of exactly `frames`, in place of the probe.
    #[must_use]
    pub const fn warmup(mut self, frames: usize) -> AllocAudit {
        self.warmup = Warmup::Fixed(frames);
        self
    }

    #[must_use]
    pub const fn frames(mut self, frames: usize) -> AllocAudit {
        self.frames = frames;
        self
    }

    /// What one measured frame may allocate. A budget above zero pins a flat cost, not an absent
    /// one: a cost that grew with the frames would pass it.
    #[must_use]
    pub const fn budget(mut self, allocs: u64) -> AllocAudit {
        self.budget = allocs;
        self
    }

    /// Warms `frame` up, then measures each frame on its own, so a growth on the nth frame fails
    /// on that frame, with its backtraces. The caller names itself in the report and the failure.
    #[track_caller]
    pub fn run(self, mut frame: impl FnMut()) -> AuditReport {
        assert!(self.frames > 0, "an audit measures at least one frame");
        let at = Location::caller();
        let warmup = match self.warmup {
            Warmup::Fixed(frames) => {
                for _ in 0..frames {
                    frame();
                }
                frames
            }
            Warmup::Probe => self.probe(&mut frame),
        };
        let mut counts = Vec::with_capacity(self.frames);
        for index in 0..self.frames {
            let window = CountingAllocator::window(&mut frame);
            if window.allocs > self.budget {
                self.fail(at, index, warmup, window);
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
            "alloc-audit {at}: worst {worst}, mode {mode}, budget {} over {} frames after {warmup} \
             of warmup",
            self.budget, self.frames,
        );
        AuditReport { worst, mode }
    }

    fn probe(self, frame: &mut impl FnMut()) -> usize {
        let mut warmup = 0;
        let mut stable = 0;
        while warmup < AllocAudit::MAX_WARMUP && stable < AllocAudit::STABLE_RUN {
            let window = CountingAllocator::window(&mut *frame);
            warmup += 1;
            stable = if window.allocs <= self.budget {
                stable + 1
            } else {
                0
            };
        }
        warmup
    }

    fn fail(
        self,
        at: &'static Location<'static>,
        index: usize,
        warmup: usize,
        mut window: AllocWindow,
    ) -> ! {
        eprintln!(
            "alloc-audit {at}: frame {index} of {} (after {warmup} of warmup) allocated {} times, \
             {} B; the budget is {} a frame",
            self.frames, window.allocs, window.bytes, self.budget,
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
            window.allocs, self.budget,
        );
    }
}

impl Default for AllocAudit {
    fn default() -> AllocAudit {
        AllocAudit::new()
    }
}
