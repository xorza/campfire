//! The allocation audit: each crate's `tests/alloc` binary installs `CountingAllocator`, and each
//! of its fixtures drives one hot path a frame at a time through `AllocAudit`, which fails on the
//! first frame past its budget, or with an allocation its budget names no site of, with that
//! frame's backtraces; and on an entry of its budget no frame makes, so an allocation a fix
//! removed leaves its budget.

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "an audit reports its frames, and a failing one each allocation's backtrace, to the terminal"
)]

use std::collections::BTreeMap;
use std::panic::Location;

use crate::alloc_trace::AllocTrace;
use crate::call_site::CallSite;
use crate::counting_allocator::{AllocWindow, CountingAllocator};

/// One audit: the frames its fixture warms up for, past the last growth of its buffers, how many
/// frames it measures, and the allocations each of those may make, each named by its site
/// (11-net-stack/testing.md, An audit holds each frame to a budget after a fixed
/// warmup). `AllocAudit::new(warmup).run(frame)` measures 64 frames, none of them allocating.
#[derive(Debug, Clone, Copy)]
pub struct AllocAudit {
    warmup: usize,
    frames: usize,
    budget: &'static [Allowed],
}

/// How a measured frame breaks its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Breach {
    /// It allocates more times than the budget allows.
    Count,
    /// An allocation of it matches no entry of the budget left to it.
    Site,
}

/// An allocation a measured frame may make: the site its call stack passes through, and why the
/// frame makes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    pub site: CallSite,
    pub why: &'static str,
}

/// What an audit measured: its worst frame, and the count most frames read, the smaller on a
/// tie.
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

    /// The allocations one measured frame may make, each matched to an allocation by its site,
    /// and each of which some frame must make: a budget pins a flat cost, not an absent one, and
    /// a cost that grew with the frames would pass it. At most `CountingAllocator::TRACE_CAP`, so
    /// each allocation it allows keeps its backtrace.
    #[must_use]
    pub const fn budget(mut self, allowed: &'static [Allowed]) -> Self {
        assert!(
            allowed.len() <= CountingAllocator::TRACE_CAP,
            "a budget names at most as many allocations as a window keeps backtraces of"
        );
        self.budget = allowed;
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
        let mut made = vec![false; self.budget.len()];
        let mut stacks = StackFits::default();
        for index in 0..self.frames {
            let mut window = CountingAllocator::window(&mut frame);
            if window.allocs > self.allowed() {
                self.fail(at, index, window, Breach::Count);
            }
            if !self.match_sites(&mut window, &mut stacks, &mut made) {
                self.fail(at, index, window, Breach::Site);
            }
            counts.push(window.allocs);
        }
        let unmade: Vec<&Allowed> = (self.budget.iter())
            .zip(&made)
            .filter(|&(_, &made)| !made)
            .map(|(allowed, _)| allowed)
            .collect();
        assert!(
            unmade.is_empty(),
            "alloc budget stale at {at}: no frame of {} made {unmade:?}; drop them",
            self.frames,
        );
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

    /// Whether each allocation of `window` can take an entry of its own whose site its call
    /// stack passes, as a match of the most allocations to the entries finds; marks in `made`
    /// each entry the match takes. A first fit would refuse a frame whose allocations all fit
    /// when one site holds another. `stacks` gives the entries of a stack an earlier frame met.
    fn match_sites(
        self,
        window: &mut AllocWindow,
        stacks: &mut StackFits,
        made: &mut [bool],
    ) -> bool {
        let mut fits = [0_u32; CountingAllocator::TRACE_CAP];
        for (fit, trace) in fits.iter_mut().zip(&mut window.traces) {
            *fit = stacks.fits(trace, self.budget);
        }
        let mut taker = [None; CountingAllocator::TRACE_CAP];
        let matched = (0..window.traces.len()).all(|trace| take(trace, &fits, &mut taker, &mut 0));
        for (made, taker) in made.iter_mut().zip(taker) {
            *made |= taker.is_some();
        }
        matched
    }

    fn fail(
        self,
        at: &'static Location<'static>,
        index: usize,
        mut window: AllocWindow,
        breach: Breach,
    ) -> ! {
        let cause = match breach {
            Breach::Count => "past the budget",
            Breach::Site => "of a site the budget names none of",
        };
        eprintln!(
            "alloc-audit {at}: frame {index} of {} (after {} of warmup) allocated {} times, {} B, \
             {cause}; the budget is {} a frame: {:?}",
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
            "alloc budget broken at {at} on frame {index}: {} allocations, {cause}, budget {}",
            window.allocs,
            self.allowed(),
        );
    }
}

/// The budget entries each call stack an audit met passes, one bit an entry, by the addresses of
/// its frames, so each stack resolves its symbols once: the stacks of a frame's allocations are
/// those of the frames before, and on Windows a resolution walks `dbghelp`, which took each net
/// audit of 256 frames past 30 s (02-engine-core.md, Allocation audits).
#[derive(Debug, Default)]
struct StackFits {
    fits: BTreeMap<Box<[usize]>, u32>,
    /// The addresses of the stack it looks up, a buffer it keeps.
    stack: Vec<usize>,
    /// The stacks it resolved, which its test counts.
    #[cfg(test)]
    resolved: usize,
}

impl StackFits {
    /// The entries of `budget` whose sites `trace`'s call stack passes, one bit an entry,
    /// resolving its symbols only for a stack it has not met.
    fn fits(&mut self, trace: &mut AllocTrace, budget: &[Allowed]) -> u32 {
        trace.stack(&mut self.stack);
        if let Some(&fits) = self.fits.get(self.stack.as_slice()) {
            return fits;
        }
        let fits = (budget.iter().enumerate())
            .filter(|(_, allowed)| trace.passes(allowed.site))
            .fold(0, |fits, (entry, _)| fits | (1 << entry));
        self.fits.insert(self.stack.as_slice().into(), fits);
        #[cfg(test)]
        {
            self.resolved += 1;
        }
        fits
    }
}

/// Whether allocation `trace` takes an entry it fits, `fits` each allocation's entries as bits,
/// that no other allocation holds or whose holder takes another in its place, `seen` the entries
/// this search has tried: Kuhn's augmenting path, writing each entry's allocation in `taker`.
fn take(
    trace: usize,
    fits: &[u32; CountingAllocator::TRACE_CAP],
    taker: &mut [Option<usize>; CountingAllocator::TRACE_CAP],
    seen: &mut u32,
) -> bool {
    (0..CountingAllocator::TRACE_CAP).any(|entry| {
        let bit = 1 << entry;
        if fits[trace] & bit == 0 || *seen & bit != 0 {
            return false;
        }
        *seen |= bit;
        let free = taker[entry].is_none_or(|holder| take(holder, fits, taker, seen));
        if free {
            taker[entry] = Some(trace);
        }
        free
    })
}

#[cfg(test)]
mod tests {
    use backtrace::Backtrace;

    use super::*;

    /// A site this test's own frames pass, and one no stack passes.
    const BUDGET: [Allowed; 2] = [
        Allowed {
            site: CallSite::Function("a_stack_resolves_once_for_every_frame_that_meets_it"),
            why: "this test's own frame",
        },
        Allowed {
            site: CallSite::Function("no_frame_has_this_name"),
            why: "a site no stack passes",
        },
    ];

    fn captured() -> AllocTrace {
        AllocTrace::new(Backtrace::new_unresolved())
    }

    #[inline(never)]
    fn captured_elsewhere() -> AllocTrace {
        captured()
    }

    #[test]
    fn a_stack_resolves_once_for_every_frame_that_meets_it() {
        // 256 traces taken at one call site share one stack: the first resolves it and finds that
        // it passes the first entry, 0b01, and the rest take that from the stacks it met. A trace
        // taken through one more frame is another stack, which resolves again, to the same entry.
        let mut stacks = StackFits::default();
        for _ in 0..256 {
            assert_eq!(stacks.fits(&mut captured(), &BUDGET), 0b01);
        }
        assert_eq!(stacks.resolved, 1);
        assert_eq!(stacks.fits(&mut captured_elsewhere(), &BUDGET), 0b01);
        assert_eq!(stacks.resolved, 2);
    }
}
