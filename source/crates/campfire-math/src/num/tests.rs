use std::cmp::Ordering;

use campfire_common::Binary;
use proptest::prelude::*;

use super::*;

/// The cases of each property a run, of new random inputs each run, as proptest draws by
/// default: a failure saves its seed under `proptest-regressions/`, which goes into version
/// control, so every later run tries it first.
pub(super) const CASES: u32 = 10_000;

pub(super) const ONE: i64 = 1 << 24;
pub(super) const HALF: i64 = 1 << 23;
pub(super) const QUARTER: i64 = 1 << 22;

pub(super) const fn n(bits: i64) -> Num {
    Num::from_bits(bits)
}

#[test]
fn int_conversion_is_exact_within_range() {
    assert_eq!(Num::ONE.to_bits(), 16_777_216);
    assert_eq!(Num::from_int(1), Some(Num::ONE));
    assert_eq!(Num::from_int(-3), Some(n(-50_331_648)));
    // 2³⁹ − 1 is the largest integer: (2³⁹ − 1) · 2²⁴ = 2⁶³ − 2²⁴.
    assert_eq!(Num::from_int((1 << 39) - 1), Some(n(i64::MAX - (ONE - 1))));
    assert_eq!(Num::from_int(-(1 << 39)), Some(Num::MIN));
    assert_eq!(Num::from_int(1 << 39), None);
    assert_eq!(Num::from_int(-(1 << 39) - 1), None);
}

#[test]
fn floor_ceil_round() {
    // (bits, floor, ceil, round); round sends halves away from zero.
    let cases = [
        (0, 0, 0, 0),
        (1, 0, 1, 0),
        (-1, -1, 0, 0),
        (HALF, 0, 1, 1),
        (-HALF, -1, 0, -1),
        (2 * ONE + HALF, 2, 3, 3),
        (-(2 * ONE + HALF), -3, -2, -3),
        (2 * ONE + HALF - 1, 2, 3, 2),
        (-(2 * ONE + HALF) + 1, -3, -2, -2),
        (i64::MAX, (1 << 39) - 1, 1 << 39, 1 << 39),
        (i64::MIN, -(1 << 39), -(1 << 39), -(1 << 39)),
        (-2 * ONE, -2, -2, -2),
    ];
    for (bits, floor, ceil, round) in cases {
        let x = n(bits);
        assert_eq!(
            (x.floor(), x.ceil(), x.round()),
            (floor, ceil, round),
            "bits {bits}"
        );
        // Exactly the whole numbers, whose floor is their ceiling, convert.
        assert_eq!(x.to_int(), (floor == ceil).then_some(floor), "bits {bits}");
    }
}

#[test]
fn add_sub_neg() {
    // 1.5 + 2.25 = 3.75; 1.5 − 2.25 = −0.75.
    assert_eq!(
        n(ONE + HALF) + n(2 * ONE + QUARTER),
        n(3 * ONE + HALF + QUARTER)
    );
    assert_eq!(n(ONE + HALF) - n(2 * ONE + QUARTER), n(-(HALF + QUARTER)));
    assert_eq!(-n(ONE), n(-ONE));
    assert_eq!(Num::MAX.checked_add(Num::EPSILON), None);
    assert_eq!(Num::MIN.checked_sub(Num::EPSILON), None);
    assert_eq!(Num::MIN.checked_neg(), None);
    assert_eq!(Num::MAX.checked_neg(), Some(n(-i64::MAX)));
}

#[test]
fn mul_rounds_to_nearest_even() {
    // (a, b, a · b in bits). ε = 2⁻²⁴, so ε · 0.5 = 0.5ε ties to 0, 1.5ε ties to 2ε, 2.5ε to 2ε.
    let cases = [
        (ONE + HALF, 2 * ONE, 3 * ONE),
        (HALF, HALF, QUARTER),
        (-(ONE + HALF), 2 * ONE, -3 * ONE),
        (1, HALF, 0),
        (3, HALF, 2),
        (5, HALF, 2),
        (-1, HALF, 0),
        (-3, HALF, -2),
        (1, HALF + QUARTER, 1),
        (1, QUARTER, 0),
        (-1, HALF + QUARTER, -1),
    ];
    for (a, b, product) in cases {
        assert_eq!(n(a) * n(b), n(product), "{a} · {b}");
        assert_eq!(n(b) * n(a), n(product), "{b} · {a}");
    }
}

#[test]
fn mul_overflow_boundary() {
    let big = Num::from_int(1 << 38).unwrap();
    let two = Num::from_int(2).unwrap();
    // 2³⁸ · −2 = −2³⁹, exactly MIN; 2³⁸ · 2 = 2³⁹ is one past MAX.
    assert_eq!(big.checked_mul(-two), Some(Num::MIN));
    assert_eq!(big.checked_mul(two), None);
}

#[test]
fn div_rounds_to_nearest_even() {
    // (a, b, a / b in bits). 1/3 = 5 592 405.33ε → 5 592 405ε; 2/3 = 11 184 810.67ε → 11 184 811ε.
    let cases = [
        (3 * ONE, 2 * ONE, ONE + HALF),
        (ONE, 4 * ONE, QUARTER),
        (-3 * ONE, 2 * ONE, -(ONE + HALF)),
        (3 * ONE, -2 * ONE, -(ONE + HALF)),
        (-3 * ONE, -2 * ONE, ONE + HALF),
        (ONE, 3 * ONE, 5_592_405),
        (2 * ONE, 3 * ONE, 11_184_811),
        (-2 * ONE, 3 * ONE, -11_184_811),
        (1, 2 * ONE, 0),
        (3, 2 * ONE, 2),
        (5, 2 * ONE, 2),
        (-3, 2 * ONE, -2),
        (1, -2 * ONE, 0),
    ];
    for (a, b, quotient) in cases {
        assert_eq!(n(a) / n(b), n(quotient), "{a} / {b}");
    }
    assert_eq!(Num::ONE.checked_div(Num::ZERO), None);
    assert_eq!(Num::MIN.checked_div(-Num::EPSILON), None);
    assert_eq!(Num::MIN.checked_div(-Num::ONE), None);
}

#[test]
fn each_rounding_takes_its_own_way() {
    // 7ε ÷ 2 = 3.5ε: to the even 4ε, down 3ε, up 4ε; −7ε ÷ 2: −4ε, −4ε, −3ε; 5ε ÷ 2 = 2.5ε: 2ε,
    // 2ε, 3ε, so the three modes give three answers over the two.
    let modes = [Rounding::NearestEven, Rounding::Floor, Rounding::Ceiling];
    let halve = |bits: i64| modes.map(|mode| n(bits).checked_mul_ratio(1, 2, mode).unwrap());
    assert_eq!(halve(7), [n(4), n(3), n(4)]);
    assert_eq!(halve(-7), [n(-4), n(-4), n(-3)]);
    assert_eq!(halve(5), [n(2), n(2), n(3)]);
    // 1 ÷ 3 = 5 592 405.33ε: 5 592 405ε to nearest and down, 5 592 406ε up; and 1.5 · 1.5 ÷ 0.75
    // = 3 whole every way.
    let third = modes.map(|mode| Num::ONE.checked_div_rounded(n(3 * ONE), mode).unwrap());
    assert_eq!(third, [n(5_592_405), n(5_592_405), n(5_592_406)]);
    let (one_half, three_quarters) = (n(ONE + HALF), n(HALF + QUARTER));
    let whole = modes.map(|mode| one_half.checked_mul_div(one_half, three_quarters, mode));
    assert_eq!(whole, [Some(n(3 * ONE)); 3]);
    // A divisor of 0, and a result past the range, give none.
    assert_eq!(
        Num::ONE.checked_div_rounded(Num::ZERO, Rounding::Floor),
        None
    );
    assert_eq!(
        Num::ONE.checked_mul_div(Num::ONE, Num::ZERO, Rounding::Floor),
        None
    );
    assert_eq!(Num::ONE.checked_mul_ratio(1, 0, Rounding::Floor), None);
    assert_eq!(Num::MAX.checked_mul_ratio(2, 1, Rounding::Floor), None);
}

#[test]
fn int_operands() {
    // 1.5 · 3 = 4.5 exactly; ε/2 ties to 0, 3ε/2 and 5ε/2 tie to 2ε.
    assert_eq!(n(ONE + HALF) * 3, n(4 * ONE + HALF));
    assert_eq!(Num::MAX.checked_mul_int(2), None);
    assert_eq!(n(1) / 2, n(0));
    assert_eq!(n(3) / 2, n(2));
    assert_eq!(n(5) / 2, n(2));
    assert_eq!(Num::ONE / 3, n(5_592_405));
    assert_eq!(-Num::ONE / 3, n(-5_592_405));
    assert_eq!(Num::ONE.checked_div_int(0), None);
    assert_eq!(Num::MIN.checked_div_int(-1), None);
    // The ends of the integer operands, past the ±1000 the proptest draws: −MIN is past MAX;
    // ε times i64::MAX is MAX's bits exactly; MIN over i64::MIN is one bit, ε; MAX over
    // i64::MIN is just short of −ε, which rounds to −ε; ONE over i64::MIN, 2⁻³⁹ bits, to 0.
    assert_eq!(Num::MIN.checked_mul_int(-1), None);
    assert_eq!(Num::EPSILON.checked_mul_int(i64::MAX), Some(Num::MAX));
    assert_eq!(Num::MIN.checked_div_int(i64::MIN), Some(n(1)));
    assert_eq!(Num::MAX.checked_div_int(i64::MIN), Some(n(-1)));
    assert_eq!(Num::ONE.checked_div_int(i64::MIN), Some(Num::ZERO));
    // ε · 1 ÷ 2 ties to 0, 3ε · 1 ÷ 2 to 2ε; ε · 1.5 ÷ 3 is ε/2, which ties to 0, where ε · 1.5
    // rounded first, to 2ε, then ÷ 3 would give ε: one rounding, not two.
    assert_eq!(n(1).checked_mul_div_int(Num::ONE, 2), Some(n(0)));
    assert_eq!(n(3).checked_mul_div_int(Num::ONE, 2), Some(n(2)));
    assert_eq!(n(1).checked_mul_div_int(n(ONE + HALF), 3), Some(n(0)));
    assert_eq!((n(1) * n(ONE + HALF)) / 3, n(1));
    assert_eq!(Num::ONE.checked_mul_div_int(Num::ONE, 0), None);
    assert_eq!(Num::MAX.checked_mul_div_int(n(2 * ONE), 1), None);
    // Each assigning operator gives what its operator does: 1.5 + 1 − 0.25 = 2.25, · 2 = 4.5,
    // / 1.5 = 3, · 3 = 9, / 4 = 2.25.
    let mut x = n(ONE + HALF);
    x += Num::ONE;
    x -= n(QUARTER);
    assert_eq!(x, n(2 * ONE + QUARTER));
    x *= n(2 * ONE);
    assert_eq!(x, n(4 * ONE + HALF));
    x /= n(ONE + HALF);
    assert_eq!(x, n(3 * ONE));
    x *= 3;
    assert_eq!(x, n(9 * ONE));
    x /= 4;
    assert_eq!(x, n(2 * ONE + QUARTER));
}

#[test]
#[should_panic(expected = "Num overflow in +")]
fn add_overflow_panics() {
    let _ = Num::MAX + Num::EPSILON;
}

#[test]
#[should_panic(expected = "Num division by zero or overflow in /")]
fn div_by_zero_panics() {
    let _ = Num::ONE / Num::ZERO;
}

#[test]
fn serializes_as_its_bits() {
    let encoded = Binary::encode(&Num::ONE);
    assert_eq!(encoded, Binary::encode(&16_777_216_i64));
    let decoded: Num = Binary::decode(&Binary::encode(&Num::MIN)).unwrap();
    assert_eq!(decoded, Num::MIN);
}

/// `numerator / denominator` rounded to nearest, ties to even, by comparing the distances to
/// the two neighbouring integers; `denominator > 0`.
pub(super) const fn nearest_even(numerator: i128, denominator: i128) -> i128 {
    let low = numerator.div_euclid(denominator);
    let high = low + 1;
    let below = numerator - low * denominator;
    let above = high * denominator - numerator;
    if below < above || (below == above && low % 2 == 0) {
        low
    } else {
        high
    }
}

/// `numerator / denominator` rounded by `rounding`, by its floor and the neighbour above:
/// the floor, the neighbour, or the nearer of the two with `nearest_even`.
fn rounded_exactly(numerator: i128, denominator: i128, rounding: Rounding) -> i128 {
    let (numerator, denominator) = if denominator < 0 {
        (-numerator, -denominator)
    } else {
        (numerator, denominator)
    };
    let floor = numerator.div_euclid(denominator);
    match rounding {
        Rounding::NearestEven => nearest_even(numerator, denominator),
        Rounding::Floor => floor,
        Rounding::Ceiling if floor * denominator == numerator => floor,
        Rounding::Ceiling => floor + 1,
    }
}

/// The number of `bits` when they fit, the oracle the exact sums and products are read by.
pub(super) fn exact(bits: i128) -> Option<Num> {
    i64::try_from(bits).ok().map(Num::from_bits)
}

pub(super) fn bits() -> impl Strategy<Value = i64> {
    prop_oneof![
        any::<i64>(),
        -(1_i64 << 40)..(1_i64 << 40),
        -64_i64..64,
        (-4096_i64..4096).prop_map(|k| k * QUARTER),
        Just(i64::MIN),
        Just(i64::MAX),
    ]
}

/// `from_raw_products` by the exact shift and the range check.
fn by_shift(sum: i128) -> Option<Num> {
    let bits = Rounding::NearestEven.shift_right(sum, Num::FRAC_BITS);
    i64::try_from(bits).ok().map(Num::from_bits)
}

#[test]
fn a_raw_sum_rounds_as_the_shift_at_its_edges() {
    // Ties either side of an odd and an even floor, the ends of `i64` in bits, and the ends of
    // `i128`, where the added half wraps.
    let half = 1_i128 << 23;
    let edge = i128::from(i64::MAX) << 24;
    let cases = [
        0,
        half,
        3 * half,
        -half,
        -3 * half,
        half + 1,
        edge,
        edge + half,
        edge + half - 1,
        -edge - (1 << 24),
        -edge - (1 << 24) - half,
        -edge - (1 << 24) - half - 1,
        i128::MAX,
        i128::MAX - half,
        i128::MIN,
        i128::MIN + half,
    ];
    for sum in cases {
        assert_eq!(Num::from_raw_products(sum), by_shift(sum), "{sum}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn a_raw_sum_rounds_as_the_shift(sum in any::<i128>(), shift in 0_u32..100) {
        let sum = sum >> shift;
        prop_assert_eq!(Num::from_raw_products(sum), by_shift(sum));
    }

    #[test]
    fn add_sub_match_exact_sums(a in bits(), b in bits()) {
        prop_assert_eq!(n(a).checked_add(n(b)), exact(i128::from(a) + i128::from(b)));
        prop_assert_eq!(n(a).checked_sub(n(b)), exact(i128::from(a) - i128::from(b)));
    }

    #[test]
    fn mul_matches_exact_rounding(a in bits(), b in bits()) {
        let expected = exact(nearest_even(i128::from(a) * i128::from(b), i128::from(ONE)));
        prop_assert_eq!(n(a).checked_mul(n(b)), expected);
    }

    #[test]
    fn div_matches_exact_rounding(a in bits(), b in bits()) {
        prop_assume!(b != 0);
        let numerator = i128::from(a) << 24;
        let expected = if b > 0 {
            nearest_even(numerator, i128::from(b))
        } else {
            nearest_even(-numerator, -i128::from(b))
        };
        prop_assert_eq!(n(a).checked_div(n(b)), exact(expected));
    }

    #[test]
    fn int_operands_match_exact(a in bits(), b in bits(), k in -1000_i64..1000) {
        prop_assert_eq!(n(a).checked_mul_int(k), exact(i128::from(a) * i128::from(k)));
        prop_assume!(k != 0);
        let product = i128::from(a) * i128::from(b);
        let divisor = i128::from(k) << Num::FRAC_BITS;
        let expected = if k > 0 {
            nearest_even(product, divisor)
        } else {
            nearest_even(-product, -divisor)
        };
        prop_assert_eq!(n(a).checked_mul_div_int(n(b), k), exact(expected));
        let expected = if k > 0 {
            nearest_even(i128::from(a), i128::from(k))
        } else {
            nearest_even(-i128::from(a), -i128::from(k))
        };
        prop_assert_eq!(n(a).checked_div_int(k), exact(expected));
    }

    #[test]
    fn each_rounding_matches_exact(
        a in bits(),
        b in bits(),
        c in bits(),
        k in -1000_i64..1000,
        m in -1000_i64..1000,
    ) {
        for rounding in [Rounding::NearestEven, Rounding::Floor, Rounding::Ceiling] {
            let rounded = |n: i128, d: i128| exact(rounded_exactly(n, d, rounding));
            let (wide_a, wide_b, wide_c) = (i128::from(a), i128::from(b), i128::from(c));
            let quotient = (b != 0).then(|| rounded(wide_a << 24, wide_b)).flatten();
            prop_assert_eq!(n(a).checked_div_rounded(n(b), rounding), quotient);
            let scaled = (c != 0).then(|| rounded(wide_a * wide_b, wide_c)).flatten();
            prop_assert_eq!(n(a).checked_mul_div(n(b), n(c), rounding), scaled);
            let ratio = (m != 0).then(|| rounded(wide_a * i128::from(k), i128::from(m))).flatten();
            prop_assert_eq!(n(a).checked_mul_ratio(k, m, rounding), ratio);
        }
    }

    #[test]
    fn floor_ceil_round_match_exact(a in bits()) {
        let floor = i128::from(a).div_euclid(i128::from(ONE));
        let twice_rest = 2 * (i128::from(a) - floor * i128::from(ONE));
        let ceil = if twice_rest == 0 { floor } else { floor + 1 };
        let up = match twice_rest.cmp(&i128::from(ONE)) {
            Ordering::Less => false,
            Ordering::Equal => a > 0,
            Ordering::Greater => true,
        };
        let round = if up { floor + 1 } else { floor };
        let x = n(a);
        prop_assert_eq!(i128::from(x.floor()), floor);
        prop_assert_eq!(i128::from(x.ceil()), ceil);
        prop_assert_eq!(i128::from(x.round()), round);
    }
}

#[test]
fn sqrt_exact_cases() {
    // √4 = 2, √2⁻²⁴ = 2⁻¹² = 4096ε, √0.25 = 0.5, √MAX fits.
    assert_eq!(n(4 * ONE).sqrt(), n(2 * ONE));
    assert_eq!(n(1).sqrt(), n(4096));
    assert_eq!(n(QUARTER).sqrt(), n(HALF));
    assert_eq!(Num::ZERO.sqrt(), Num::ZERO);
    assert_eq!(n(-1).checked_sqrt(), None);
    // √(2³⁹ − ε) is 741455.2, 12 439 554 047 902 bits rounded to nearest.
    assert_eq!(Num::MAX.sqrt(), n(12_439_554_047_902));
    // The widest root that fits: (2⁶³ − ½)² = 2¹²⁶ − 2⁶³ + ¼, so 2¹²⁶ − 2⁶³ rounds down to
    // 2⁶³ − 1, and one more rounds up to 2⁶³, which does not fit, as no value from 2¹²⁶ on does.
    assert_eq!(
        Num::from_root_of_bits((1 << 126) - (1 << 63)),
        Some(Num::MAX)
    );
    assert_eq!(Num::from_root_of_bits((1 << 126) - (1 << 63) + 1), None);
    assert_eq!(Num::from_root_of_bits(1 << 126), None);
    assert_eq!(Num::from_root_of_bits(u128::MAX), None);
}

#[test]
#[should_panic(expected = "Num square root of a negative value")]
fn sqrt_of_negative_panics() {
    let _root = n(-ONE).sqrt();
}

fn sqrt_rounds_to_nearest(a: i64) -> bool {
    let root = u128::from(n(a).sqrt().to_bits().cast_unsigned());
    let four_scaled = u128::from(a.cast_unsigned()) << 26;
    (2 * root).saturating_sub(1).pow(2) <= four_scaled && four_scaled < (2 * root + 1).pow(2)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn sqrt_is_nearest(a in 0_i64..=i64::MAX) {
        prop_assert!(sqrt_rounds_to_nearest(a));
    }
}
