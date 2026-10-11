use proptest::prelude::*;

use super::*;
/// The cases of each property a run, as the other tests of the crate draw.
const CASES: u32 = 10_000;

/// The division by Rust's `u128` operators.
fn reference(numerator: u128, divisor: u128) -> WideDivision {
    WideDivision {
        quotient: numerator / divisor,
        rest: numerator % divisor,
    }
}

#[test]
fn each_way_divides_exactly_at_the_edges() {
    // The smallest and largest divisors of 64 bits with the largest numerator whose quotient fits
    // 64 bits, `d · 2⁶⁴ − 1`, and one past it; exact multiples, whose rest is 0; a rest of
    // `d − 1`; and divisors past 64 bits.
    let cases: [(u128, u128); 12] = [
        (0, 1),
        ((1 << 64) - 1, 1),
        ((u128::from(u64::MAX) << 64) - 1, u64::MAX.into()),
        ((1 << 64) * 3 - 1, 3),
        (u128::from(u64::MAX) * 12_345, u64::MAX.into()),
        (u128::from(u64::MAX) * 12_345 - 1, u64::MAX.into()),
        ((1 << 100) + 7, (1 << 40) + 1),
        (1 << 63, 1 << 63),
        (1 << 64, 1),
        (u128::MAX, u128::from(u64::MAX)),
        (u128::MAX, 1 << 64),
        (u128::MAX, u128::MAX - 1),
    ];
    for (numerator, divisor) in cases {
        let expected = reference(numerator, divisor);
        assert_eq!(
            WideDivision::of(numerator, divisor),
            expected,
            "{numerator} / {divisor}"
        );
        assert_eq!(
            aarch64::divide(numerator, divisor),
            expected,
            "{numerator} / {divisor}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn each_way_is_the_division(divisor in 1_u64.., quotient in any::<u64>(), rest in any::<u64>(), shift in 0_u32..64) {
        let divisor = u128::from(divisor >> shift | 1);
        let numerator = u128::from(quotient) * divisor + u128::from(rest) % divisor;
        let expected = reference(numerator, divisor);
        prop_assert_eq!(WideDivision::of(numerator, divisor), expected);
        prop_assert_eq!(aarch64::divide(numerator, divisor), expected);
    }
}
