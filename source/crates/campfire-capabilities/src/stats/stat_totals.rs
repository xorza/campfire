use campfire_math::{Num, U256};

use crate::stats::stat_op::StatOp;

/// What a stat's value sums from its base and its modifiers, in bits: the base and each add, each
/// percent, and the largest cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatTotals {
    add: i128,
    pct: i128,
    cut: i128,
}

impl StatTotals {
    /// The totals of a stat whose base is `base` bits, before any change.
    pub(crate) const fn base(base: i128) -> StatTotals {
        StatTotals {
            add: base,
            pct: 0,
            cut: 0,
        }
    }

    /// Adds a change of `change` bits by `op`: an add or a percent sums, and only the largest cut
    /// counts.
    pub(crate) const fn change(&mut self, op: StatOp, change: i128) {
        match op {
            StatOp::Add => self.add += change,
            StatOp::Pct => self.pct += change,
            StatOp::Cut if change > self.cut => self.cut = change,
            StatOp::Cut => {}
        }
    }

    /// `(add) × (1 + pct) × (1 − cut)`, the cut from 0 to 1, each sum stopped at the range of a
    /// number, the product rounded once to nearest and stopped at that range too.
    pub(crate) fn value(self) -> Num {
        let one = 1_i128 << Num::FRAC_BITS;
        let base = saturate(self.add).to_bits();
        let gain = saturate(one + self.pct).to_bits();
        let kept = one - self.cut.clamp(0, one);
        let negative = (base < 0) != (gain < 0);
        let magnitude = u128::from(base.unsigned_abs()) * u128::from(gain.unsigned_abs());
        let product = U256::product(magnitude, kept.cast_unsigned())
            .round_shr(2 * Num::FRAC_BITS)
            .map_or(i128::MAX, |bits| i128::try_from(bits).unwrap_or(i128::MAX));
        saturate(if negative { -product } else { product })
    }
}

/// `bits` as a number, at the end of the range of numbers when past it.
fn saturate(bits: i128) -> Num {
    let bits = bits.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
    Num::from_bits(i64::try_from(bits).expect("clamped to the range"))
}
