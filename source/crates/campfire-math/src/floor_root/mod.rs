#[cfg(feature = "bench")]
pub(crate) mod bench;

/// The exact integer square root, rounded down. A float gives only the first estimate, which
/// integer steps correct whatever it is, so the result never depends on the float.
pub trait FloorRoot {
    /// The largest integer whose square is at most `self`, lane by lane for lanes.
    #[must_use]
    fn floor_root(self) -> Self;
}

/// The integer square root nearest to the exact one, for values below 2¹²⁶, whose roots are
/// below 2⁶³. A root is never exactly a half, as `(k + ½)²` is not an integer.
pub(crate) trait NearestRoot {
    #[must_use]
    fn nearest_root(self) -> Self;
}

impl FloorRoot for u128 {
    /// Each path's estimate, truncated, lies within one of the root's floor, and is that floor
    /// unless the root lies within the estimate's error of an integer: a square's float root is
    /// exact, so it is no such case. Each path's loops take that one rare step, which the CPU
    /// predicts as not taken, so a caller that waits on the root waits on no correction.
    #[expect(
        clippy::inline_always,
        reason = "left to the inliner, the call stays, and costs `atomic/floor_root/narrow` 10 % on a Ryzen and 22 % on an M2"
    )]
    #[inline(always)]
    fn floor_root(self) -> u128 {
        u128::from(if let Ok(narrow) = u64::try_from(self) {
            narrow_root(narrow)
        } else if self < NEAR_END {
            Near::of(self).floor()
        } else {
            Far::of(self).floor()
        })
    }
}

impl NearestRoot for u128 {
    /// Each path's estimate, rounded, `n`, which is the nearest root unless the root lies within
    /// the estimate's error of a half. `n` is the nearest exactly when `n² − n < value ≤ n² + n`,
    /// as `(n ± ½)² = n² ± n + ¼`; the rare steps correct the rest, as in `floor_root`.
    #[expect(
        clippy::inline_always,
        reason = "left to the inliner, the call stays, and costs `atomic/floor_root/narrow` 10 % on a Ryzen and 22 % on an M2"
    )]
    #[inline(always)]
    fn nearest_root(self) -> u128 {
        debug_assert!(self < 1 << 126);
        u128::from(if self < NEAR_END {
            Near::of(self).nearest()
        } else {
            Far::of(self).nearest()
        })
    }
}

/// The floor root of a value below 2⁶⁴, on `u64`, whose steps are single instructions: the float
/// root of a value below 2⁵³ is that of the exact value, and above it the value rounds once,
/// which leaves the estimate within 1 of the root, and the two loops step it there.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the f64 root is only an estimate; the integer steps fix the result exactly"
)]
fn narrow_root(value: u64) -> u64 {
    // The root is below 2³² + 1, which the signed cast takes in one instruction on x86-64.
    let mut root = (value as f64).sqrt() as i64 as u64;
    // A root of 2³² squares past `u64`, which `checked_mul` counts as above the value.
    while root.checked_mul(root).is_none_or(|square| square > value) {
        root -= 1;
    }
    // (root + 1)² ≤ value exactly when value − root² > 2·root, which cannot overflow once
    // root² is at most the value: the wrapping steps skip the checks a release build makes.
    while value.wrapping_sub(root.wrapping_mul(root)) > 2 * root {
        root += 1;
    }
    root
}

/// The end of the values `Near` takes, 2¹⁰², where roots reach 2⁵¹.
const NEAR_END: u128 = 1 << 102;
/// 2⁵⁰, exact in f64: the split of a value below 2¹⁰² into two parts that convert exactly.
const TWO_POW_50: f64 = 1_125_899_906_842_624.0;
/// 2⁵², whose last place is 1: a float from 0 to below 2⁵² added to it rounds to the nearest
/// integer, ties to even, which fills its low bits.
const TWO_POW_52: f64 = 4_503_599_627_370_496.0;
/// 2⁵³, exact in f64.
const TWO_POW_53: f64 = 9_007_199_254_740_992.0;
/// 2¹¹, exact in f64: the root of the 2²² of the bits `Far` drops.
const TWO_POW_11: f64 = 2048.0;
/// 2³², exact in f64.
const TWO_POW_32: f64 = 4_294_967_296.0;
/// 2¹⁵, exact in f64.
const TWO_POW_15: f64 = 32_768.0;
/// 2⁻¹², exact in f64.
const TWO_POW_MINUS_12: f64 = 1.0 / 4096.0;

/// A value below 2¹⁰² and its float root, which lies within 1.5 · 2⁻⁵³ of the root, below
/// 2⁵¹: within 0.375. Its two parts convert to `f64` exactly, each below 2⁵², and only their sum
/// rounds, by at most 2⁻⁵³ of the value, as the product of the fused multiply-add is exact; the
/// root halves that and rounds by at most 2⁻⁵³ of itself. A root `r` within one of the answer
/// leaves `value − r²` below 2⁵³ in magnitude, so its low 64 bits, wrapped, are that residual,
/// and every correction runs on `u64`.
#[derive(Debug, Clone, Copy)]
struct Near {
    value: u64,
    estimate: f64,
}

impl Near {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "the parts below 2⁵², and the value's low bits, which the residuals keep"
    )]
    fn of(value: u128) -> Near {
        debug_assert!(value < NEAR_END);
        let high = (value >> 50) as i64 as f64;
        let low = (value & ((1 << 50) - 1)) as i64 as f64;
        Near {
            value: value as u64,
            estimate: high.mul_add(TWO_POW_50, low).sqrt(),
        }
    }

    /// `value − root²`, exact for a root within one of the answer.
    const fn residual(self, root: u64) -> i64 {
        self.value
            .wrapping_sub(root.wrapping_mul(root))
            .cast_signed()
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an estimate below 2⁵¹ + 1, which the cast truncates exactly"
    )]
    fn floor(self) -> u64 {
        let mut root = self.estimate as i64 as u64;
        while self.residual(root) < 0 {
            root -= 1;
        }
        // (root + 1)² ≤ value exactly when value − root² > 2·root.
        while self.residual(root) > 2 * root.cast_signed() {
            root += 1;
        }
        root
    }

    #[expect(
        clippy::float_arithmetic,
        reason = "the rounding of an estimate that integer steps correct"
    )]
    fn nearest(self) -> u64 {
        let mut root = (self.estimate + TWO_POW_52).to_bits() - TWO_POW_52.to_bits();
        loop {
            let (rest, reach) = (self.residual(root), root.cast_signed());
            if rest > reach {
                root += 1;
            } else if rest <= -reach && root > 0 {
                root -= 1;
            } else {
                return root;
            }
        }
    }
}

/// A value of 2¹⁰² or more and an estimate of its root, from 2⁵¹ to 2⁶⁴ − 1, `base + offset`,
/// within 2⁻¹⁹ of the root: an integer that holds the bits a float cannot, and a float from 0
/// to 2¹⁶. The float root `e` of the value, its top bits converted and summed with one rounding,
/// lies within 2⁻⁵¹ of the root, so within 2¹³, and `m`, `e` to a multiple of 2¹² held below 2⁶⁴,
/// within 2¹⁴. The residual `d = value − m²` takes `m` to `m + d / 2m`, short of the root by
/// `d² / 8m³` at most, below 2⁻²⁰ here. `d`, below 2⁸⁰ in magnitude, converts as two exact parts
/// with one rounding; the quotient takes `e` for `m`, 2⁻³⁷ of it apart, and its floats round
/// within 2⁻³⁰ of it. So the estimate needs no integer division, and its float quotient starts
/// as soon as `e` is known, beside the integer steps.
#[derive(Debug, Clone, Copy)]
struct Far {
    value: u128,
    base: u64,
    offset: f64,
}

impl Far {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::float_arithmetic,
        reason = "the f64 root is only an estimate, which integer steps correct"
    )]
    #[inline(never)]
    fn of(value: u128) -> Far {
        debug_assert!(value >= NEAR_END);
        // The value's bits from 2²² up, as two parts below 2⁵³ that convert exactly, as a single
        // instruction each: the bits dropped are below 2⁻⁸⁰ of the value.
        let high = (value >> 75) as i64 as f64;
        let low = ((value >> 22) as i64 & ((1 << 53) - 1)) as f64;
        let estimate = high.mul_add(TWO_POW_53, low).sqrt() * TWO_POW_11;
        let half_reciprocal = 0.5 / estimate;
        // `e` · 2⁻¹² is at most 2⁵², which the rounding still takes, and the bound keeps `m`
        // below 2⁶⁴; one at 2⁶⁴ − 2¹² lies within 2¹² of a root near 2⁶⁴.
        let coarse = (estimate.mul_add(TWO_POW_MINUS_12, TWO_POW_52).to_bits()
            - TWO_POW_52.to_bits())
        .min((1 << 52) - 1);
        let base = coarse << 12;
        let residual = value
            .wrapping_sub(u128::from(base) * u128::from(base))
            .cast_signed();
        let residual =
            ((residual >> 32) as i64 as f64).mul_add(TWO_POW_32, f64::from(residual as u32));
        // The step is within 2¹⁵ either side of 0; the estimate takes it 2¹⁵ up, from a base 2¹⁵
        // down, so its offset is never negative, and the sum stays exact in its last place.
        Far {
            value,
            base: base - (1 << 15),
            offset: residual.mul_add(half_reciprocal, TWO_POW_15),
        }
    }

    /// `root²`, which fits `u128` for every root below 2⁶⁴.
    const fn square(root: u64) -> u128 {
        root as u128 * root as u128
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the offset is from 0 to 2¹⁶, which the cast truncates exactly"
    )]
    fn floor(self) -> u64 {
        let mut root = self.base.saturating_add(self.offset as i64 as u64);
        while Far::square(root) > self.value {
            root -= 1;
        }
        // (root + 1)² ≤ value exactly when value − root² > 2·root, which cannot overflow, as
        // root² is at most the value; 2·root is below 2⁶⁵.
        while self.value - Far::square(root) > 2 * u128::from(root) {
            root += 1;
        }
        root
    }

    #[expect(
        clippy::float_arithmetic,
        reason = "the rounding of an estimate that integer steps correct"
    )]
    fn nearest(self) -> u64 {
        let rounded = (self.offset + TWO_POW_52).to_bits() - TWO_POW_52.to_bits();
        let mut root = self.base.saturating_add(rounded);
        // Below 2¹²⁶, roots are below 2⁶³, so the residual is within `i128`.
        loop {
            let rest = self
                .value
                .cast_signed()
                .wrapping_sub(Far::square(root).cast_signed());
            let reach = i128::from(root);
            if rest > reach {
                root += 1;
            } else if rest <= -reach {
                root -= 1;
            } else {
                return root;
            }
        }
    }
}

#[cfg(test)]
mod tests;
