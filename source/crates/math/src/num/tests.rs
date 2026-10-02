use std::cmp::Ordering;

use proptest::prelude::*;

use super::*;

const ONE: i64 = 1 << 24;
const HALF: i64 = 1 << 23;
const QUARTER: i64 = 1 << 22;

const fn n(bits: i64) -> Num {
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
    // ε · 1 ÷ 2 ties to 0, 3ε · 1 ÷ 2 to 2ε; ε · 1.5 ÷ 3 is ε/2, which ties to 0, where ε · 1.5
    // rounded first, to 2ε, then ÷ 3 would give ε: one rounding, not two.
    assert_eq!(n(1).checked_mul_div_int(Num::ONE, 2), Some(n(0)));
    assert_eq!(n(3).checked_mul_div_int(Num::ONE, 2), Some(n(2)));
    assert_eq!(n(1).checked_mul_div_int(n(ONE + HALF), 3), Some(n(0)));
    assert_eq!((n(1) * n(ONE + HALF)) / 3, n(1));
    assert_eq!(Num::ONE.checked_mul_div_int(Num::ONE, 0), None);
    assert_eq!(Num::MAX.checked_mul_div_int(n(2 * ONE), 1), None);
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
const fn nearest_even(numerator: i128, denominator: i128) -> i128 {
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
        prop_assert_eq!(n(a).checked_add(n(b)), narrow(i128::from(a) + i128::from(b)));
        prop_assert_eq!(n(a).checked_sub(n(b)), narrow(i128::from(a) - i128::from(b)));
    }

    #[test]
    fn mul_matches_exact_rounding(a in bits(), b in bits()) {
        let expected = narrow(nearest_even(i128::from(a) * i128::from(b), i128::from(ONE)));
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
        prop_assert_eq!(n(a).checked_div(n(b)), narrow(expected));
    }

    #[test]
    fn int_operands_match_exact(a in bits(), b in bits(), k in -1000_i64..1000) {
        prop_assert_eq!(n(a).checked_mul_int(k), narrow(i128::from(a) * i128::from(k)));
        prop_assume!(k != 0);
        let product = i128::from(a) * i128::from(b);
        let divisor = i128::from(k) << Num::FRAC_BITS;
        let expected = if k > 0 {
            nearest_even(product, divisor)
        } else {
            nearest_even(-product, -divisor)
        };
        prop_assert_eq!(n(a).checked_mul_div_int(n(b), k), narrow(expected));
        let expected = if k > 0 {
            nearest_even(i128::from(a), i128::from(k))
        } else {
            nearest_even(-i128::from(a), -i128::from(k))
        };
        prop_assert_eq!(n(a).checked_div_int(k), narrow(expected));
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
fn parse_reads_exact_decimals() {
    // 2⁻²⁴ = 0.000000059604644775390625, 2⁻²⁵ = 0.0000000298023223876953125 (25 digits).
    let cases = [
        ("0", 0),
        ("-0", 0),
        ("1", ONE),
        ("7.5", 7 * ONE + HALF),
        ("-0.25", -QUARTER),
        ("0.000000059604644775390625", 1),
        // Exactly half of ε ties to the even 0; any later non-zero digit moves it above the tie.
        ("0.0000000298023223876953125", 0),
        ("0.00000002980232238769531250000000001", 1),
        ("0.0000000298023223876953124999999999", 0),
        // 1.5ε ties to the even 2ε; −0.5ε ties to 0.
        ("0.0000000894069671630859375", 2),
        ("-0.0000000298023223876953125", 0),
        // 2³⁹ − 2⁻²⁴ and −2³⁹ are MAX and MIN.
        ("549755813887.999999940395355224609375", i64::MAX),
        // The midpoint between MAX and 2³⁹ is 2³⁹ − 2.98…·10⁻⁸; 3·10⁻⁸ below 2³⁹ is under it.
        ("549755813887.99999997", i64::MAX),
        ("-549755813888", i64::MIN),
        // Less than half an ε below −2³⁹ still rounds to MIN.
        ("-549755813888.00000000000000000000000001", i64::MIN),
        ("000123.500", 123 * ONE + HALF),
    ];
    for (text, bits) in cases {
        assert_eq!(text.parse::<Num>(), Ok(n(bits)), "{text}");
    }
}

#[test]
fn parse_rejects_bad_text() {
    for text in [
        "", "-", "+1", "1.", ".5", "1.2.3", " 1", "1 ", "1e3", "1_000", "--1", "0x10",
    ] {
        assert_eq!(
            text.parse::<Num>(),
            Err(ParseNumError::Malformed),
            "{text:?}"
        );
    }
    for text in [
        "549755813888",
        "549755813887.99999998",
        "-549755813888.00000003",
        "99999999999999999999999999999999999999999",
    ] {
        assert_eq!(
            text.parse::<Num>(),
            Err(ParseNumError::OutOfRange),
            "{text}"
        );
    }
}

#[test]
fn display_is_exact() {
    let cases = [
        (0, "0"),
        (ONE, "1"),
        (7 * ONE + HALF, "7.5"),
        (-QUARTER, "-0.25"),
        (1, "0.000000059604644775390625"),
        (-1, "-0.000000059604644775390625"),
        (i64::MAX, "549755813887.999999940395355224609375"),
        (i64::MIN, "-549755813888"),
    ];
    for (bits, text) in cases {
        assert_eq!(n(bits).to_string(), text);
    }
    assert_eq!(
        format!("{:>6}|{:<6}|", Num::ONE, Num::ONE),
        "     1|1     |"
    );
}

#[test]
fn constants_match_long_decimals() {
    // Machin's formula and the parser must agree on the rounded value.
    let cases = [
        (Num::PI, "3.1415926535897932384626433832795028841971"),
        (Num::TAU, "6.2831853071795864769252867665590057683943"),
        (Num::FRAC_PI_2, "1.5707963267948966192313216916397514420985"),
    ];
    for (constant, text) in cases {
        assert_eq!(Ok(constant), text.parse::<Num>(), "{text}");
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
}

#[test]
#[should_panic(expected = "Num square root of a negative value")]
fn sqrt_of_negative_panics() {
    let _root = n(-ONE).sqrt();
}

#[test]
fn trig_exact_points() {
    let quarter_pi = "0.78539816339744830961566084581987572104929"
        .parse::<Num>()
        .unwrap();
    assert_eq!(
        Num::ZERO.sin_cos(),
        SinCos {
            sin: Num::ZERO,
            cos: Num::ONE
        }
    );
    assert_eq!(
        Num::PI.sin_cos(),
        SinCos {
            sin: Num::ZERO,
            cos: -Num::ONE
        }
    );
    assert_eq!(
        Num::FRAC_PI_2.sin_cos(),
        SinCos {
            sin: Num::ONE,
            cos: Num::ZERO
        }
    );
    assert_eq!(Num::ZERO.atan2(Num::ZERO), Num::ZERO);
    assert_eq!(Num::ZERO.atan2(Num::ONE), Num::ZERO);
    assert_eq!(Num::ZERO.atan2(-Num::ONE), Num::PI);
    assert_eq!(Num::ONE.atan2(Num::ZERO), Num::FRAC_PI_2);
    assert_eq!((-Num::ONE).atan2(Num::ZERO), -Num::FRAC_PI_2);
    assert_eq!(Num::ONE.atan2(Num::ONE), quarter_pi);
}

#[expect(
    clippy::cast_precision_loss,
    reason = "test values stay below 2⁵³, where f64 is exact"
)]
#[expect(
    clippy::float_arithmetic,
    reason = "f64 is only the accuracy reference"
)]
fn to_f64(x: Num) -> f64 {
    x.to_bits() as f64 / 16_777_216.0
}

#[expect(
    clippy::cast_precision_loss,
    reason = "test values stay below 2⁵³, where f64 is exact"
)]
#[expect(
    clippy::float_arithmetic,
    reason = "f64 is only the accuracy reference"
)]
fn ulp_error(result: Num, reference: f64) -> f64 {
    (result.to_bits() as f64 - reference * 16_777_216.0).abs()
}

/// Largest error in ulp of `sin_cos` over evenly spaced angles in `[from, to)`.
fn sin_cos_max_ulp(from: i64, to: i64, count: i64) -> f64 {
    let mut worst = 0.0_f64;
    for i in 0..count {
        let angle = n(from + (to - from) / count * i);
        let exact = to_f64(angle);
        let result = angle.sin_cos();
        worst = worst.max(ulp_error(result.sin, exact.sin()));
        worst = worst.max(ulp_error(result.cos, exact.cos()));
    }
    worst
}

/// Largest error in ulp of `atan2` over a grid of points in `[−range, range]²`.
fn atan2_max_ulp(range: i64, steps: i64) -> f64 {
    let mut worst = 0.0_f64;
    for i in 0..=steps {
        for j in 0..=steps {
            let y = n(-range + 2 * range / steps * i + 7);
            let x = n(-range + 2 * range / steps * j + 3);
            worst = worst.max(ulp_error(y.atan2(x), to_f64(y).atan2(to_f64(x))));
        }
    }
    worst
}

#[test]
fn sin_cos_within_bound() {
    let worst = sin_cos_max_ulp(-8 * ONE, 8 * ONE, 200_000).max(sin_cos_max_ulp(
        -(1 << 52),
        1 << 52,
        20_000,
    ));
    assert!(worst <= SIN_COS_ULP, "sin_cos off by {worst} ulp");
}

#[test]
fn atan2_within_bound() {
    let worst = atan2_max_ulp(50 * ONE, 400).max(atan2_max_ulp(ONE / 64, 200));
    assert!(worst <= ATAN2_ULP, "atan2 off by {worst} ulp");
}

const SIN_COS_ULP: f64 = 0.501;
const ATAN2_ULP: f64 = 0.501;

fn sqrt_rounds_to_nearest(a: i64) -> bool {
    let root = u128::from(n(a).sqrt().to_bits().cast_unsigned());
    let four_scaled = u128::from(a.cast_unsigned()) << 26;
    (2 * root).saturating_sub(1).pow(2) <= four_scaled && four_scaled < (2 * root + 1).pow(2)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn display_parses_back(a in bits()) {
        prop_assert_eq!(n(a).to_string().parse::<Num>(), Ok(n(a)));
    }

    #[test]
    fn parse_matches_exact_rounding(
        negative in any::<bool>(),
        int in 0_u64..1_000_000_000_000,
        frac in "[0-9]{0,18}",
    ) {
        let text = format!("{}{int}{}{frac}", if negative { "-" } else { "" }, if frac.is_empty() { "" } else { "." });
        let scale = 10_i128.pow(u32::try_from(frac.len()).unwrap());
        let digits = i128::from(int) * scale + frac.parse::<i128>().unwrap_or(0);
        let magnitude = nearest_even(digits << 24, scale);
        let expected = narrow(if negative { -magnitude } else { magnitude });
        prop_assert_eq!(text.parse::<Num>().ok(), expected);
    }

    #[test]
    fn sqrt_is_nearest(a in 0_i64..=i64::MAX) {
        prop_assert!(sqrt_rounds_to_nearest(a));
    }

    #[test]
    fn trig_symmetries(a in -(1_i64 << 45)..(1_i64 << 45), b in bits()) {
        let plus = n(a).sin_cos();
        let minus = (-n(a)).sin_cos();
        prop_assert_eq!(minus, SinCos { sin: -plus.sin, cos: plus.cos });
        prop_assume!(a != i64::MIN);
        prop_assert_eq!((-n(a)).atan2(n(b)), -n(a).atan2(n(b)));
    }
}
