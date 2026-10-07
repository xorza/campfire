#[cfg(feature = "bench")]
pub(crate) mod bench;

/// The exact integer square root, rounded down. A float gives only the first estimate, which
/// integer steps correct whatever it is, so the result never depends on the float.
pub trait FloorRoot {
    /// The largest integer whose square is at most `self`.
    #[must_use]
    fn floor_root(self) -> Self;
}

impl FloorRoot for u128 {
    fn floor_root(self) -> u128 {
        // Every root is below 2⁶⁴, so a caller that squares it again takes one widening multiply.
        u128::from(match u64::try_from(self) {
            Ok(narrow) => narrow_root(narrow),
            Err(_) => wide_root(self),
        })
    }
}

/// 2⁶⁴, exact in f64.
const TWO_POW_64: f64 = 18_446_744_073_709_551_616.0;

/// The floor root of a value below 2⁶⁴, on `u64`, whose steps are single instructions. The
/// float holds a value below 2⁵³ exactly, and above it rounds once, which leaves the estimate
/// within 1 of the root.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the f64 root is only an estimate; the integer steps fix the result exactly"
)]
fn narrow_root(value: u64) -> u64 {
    let mut root = (value as f64).sqrt() as u64;
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

/// The floor root of a value of 2⁶⁴ or more, from 2³² to 2⁶⁴ − 1. Converting the two halves
/// takes one instruction each, where converting a `u128` is a library call.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_arithmetic,
    reason = "the f64 root is only an estimate; the integer steps fix the result exactly"
)]
fn wide_root(value: u128) -> u64 {
    let high = (value >> 64) as u64;
    let low = value as u64;
    let estimate = (high as f64 * TWO_POW_64 + low as f64).sqrt();
    // The cast saturates: an estimate of 2⁶⁴, which a value near 2¹²⁸ gives, becomes 2⁶⁴ − 1.
    let mut root = estimate as u64;
    // Above 2⁵² the estimate can be off by up to 2¹⁰; one Newton step brings it within 1. Its
    // result is at most 2⁶⁴, held to 2⁶⁴ − 1, the largest root.
    if root > 1 << 52 {
        let wide = u128::from(root);
        root = u64::try_from(wide.midpoint(value / wide)).unwrap_or(u64::MAX);
    }
    let square = |root: u64| u128::from(root) * u128::from(root);
    while square(root) > value {
        root -= 1;
    }
    // As in `narrow_root`; 2·root is below 2⁶⁵.
    while value.wrapping_sub(square(root)) > 2 * u128::from(root) {
        root += 1;
    }
    root
}

#[cfg(test)]
mod tests;
