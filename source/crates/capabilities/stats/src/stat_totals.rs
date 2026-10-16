use campfire_math::Num;

use crate::stat_op::StatOp;

/// What a stat's value sums from its base and its modifiers, in bits: the base and each add, each
/// percent, and the largest cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatTotals {
    add: i128,
    pct: i128,
    cut: i128,
}

impl StatTotals {
    /// The totals of a stat whose base is `base` bits, before any change.
    pub(crate) const fn base(base: i128) -> Self {
        Self {
            add: base,
            pct: 0,
            cut: 0,
        }
    }

    /// Adds a change of `change` bits by `op`: an add or a percent sums, and only the largest cut
    /// counts.
    pub const fn change(&mut self, op: StatOp, change: i128) {
        match op {
            StatOp::Add => self.add += change,
            StatOp::Pct => self.pct += change,
            StatOp::Cut if change > self.cut => self.cut = change,
            StatOp::Cut => {}
        }
    }

    /// `(add) × (1 + pct) × (1 − cut)`, the cut from 0 to 1, each sum and each product stopped
    /// at the range of a number, each product rounded to nearest: within a bit of the product
    /// rounded once (design 09, D11).
    pub fn value(self) -> Num {
        let one = 1_i128 << Num::FRAC_BITS;
        let kept = Num::saturating_from_bits(one - self.cut.clamp(0, one));
        Num::saturating_from_bits(self.add)
            .saturating_mul(Num::saturating_from_bits(one + self.pct))
            .saturating_mul(kept)
    }
}
