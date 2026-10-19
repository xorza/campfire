//! A byte or event counter that a bench reads.
//!
//! The cell holds its value only in `internals` builds and is zero-sized otherwise. Its mutators
//! are unconditional, so call sites need no `#[cfg]`, and a shipping build stores and computes
//! nothing for it. It never allocates, so the allocation suite, which builds with `internals`,
//! still measures the frame alone. A counter accumulates for the life of its owner, and readers
//! subtract two readings.

use core::ops::AddAssign;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BenchCounter {
    #[cfg(feature = "internals")]
    value: u64,
}

impl BenchCounter {
    /// A counter that starts at `amount`.
    pub(crate) fn new(amount: usize) -> Self {
        let mut counter = Self::default();
        counter.add(amount);
        counter
    }

    #[cfg_attr(
        not(feature = "internals"),
        expect(
            clippy::needless_pass_by_ref_mut,
            reason = "the `internals` build writes through the reference"
        )
    )]
    pub(crate) const fn add(&mut self, amount: usize) {
        #[cfg(feature = "internals")]
        {
            self.value = self.value.saturating_add(amount as u64);
        }
        #[cfg(not(feature = "internals"))]
        let _ = amount;
    }

    #[cfg(feature = "internals")]
    pub(crate) const fn get(self) -> u64 {
        self.value
    }
}

impl AddAssign for BenchCounter {
    #[cfg_attr(
        not(feature = "internals"),
        expect(unused_variables, reason = "the `internals` build reads `rhs`")
    )]
    fn add_assign(&mut self, rhs: Self) {
        #[cfg(feature = "internals")]
        {
            self.value = self.value.saturating_add(rhs.value);
        }
    }
}
