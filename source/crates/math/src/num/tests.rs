use std::cmp::Ordering;

use proptest::prelude::*;

use super::*;

const ONE: i64 = 1 << 24;
const HALF: i64 = 1 << 23;
const QUARTER: i64 = 1 << 22;

fn n(bits: i64) -> Num {
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
    ];
    for (bits, floor, ceil, round) in cases {
        let x = n(bits);
        assert_eq!(
            (x.floor(), x.ceil(), x.round()),
            (floor, ceil, round),
            "bits {bits}"
        );
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
    let encoded = postcard::to_allocvec(&Num::ONE).unwrap();
    assert_eq!(encoded, postcard::to_allocvec(&16_777_216_i64).unwrap());
    let decoded: Num = postcard::from_bytes(&postcard::to_allocvec(&Num::MIN).unwrap()).unwrap();
    assert_eq!(decoded, Num::MIN);
}

/// `numerator / denominator` rounded to nearest, ties to even, by comparing the distances to
/// the two neighbouring integers; `denominator > 0`.
fn nearest_even(numerator: i128, denominator: i128) -> i128 {
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

fn to_num(bits: i128) -> Option<Num> {
    i64::try_from(bits).ok().map(Num::from_bits)
}

fn bits() -> impl Strategy<Value = i64> {
    prop_oneof![
        any::<i64>(),
        -(1_i64 << 40)..(1_i64 << 40),
        -64_i64..64,
        (-4096_i64..4096).prop_map(|k| k * QUARTER),
        Just(i64::MIN),
        Just(i64::MAX),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn add_sub_match_exact_sums(a in bits(), b in bits()) {
        prop_assert_eq!(n(a).checked_add(n(b)), to_num(i128::from(a) + i128::from(b)));
        prop_assert_eq!(n(a).checked_sub(n(b)), to_num(i128::from(a) - i128::from(b)));
    }

    #[test]
    fn mul_matches_exact_rounding(a in bits(), b in bits()) {
        let expected = to_num(nearest_even(i128::from(a) * i128::from(b), i128::from(ONE)));
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
        prop_assert_eq!(n(a).checked_div(n(b)), to_num(expected));
    }

    #[test]
    fn int_operands_match_exact(a in bits(), k in -1000_i64..1000) {
        prop_assert_eq!(n(a).checked_mul_int(k), to_num(i128::from(a) * i128::from(k)));
        prop_assume!(k != 0);
        let expected = if k > 0 {
            nearest_even(i128::from(a), i128::from(k))
        } else {
            nearest_even(-i128::from(a), -i128::from(k))
        };
        prop_assert_eq!(n(a).checked_div_int(k), to_num(expected));
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
