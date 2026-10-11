use proptest::prelude::*;

use super::*;
use crate::num::tests::{CASES, ONE, bits, n};

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
    // The ends of the range, where a ratio of the two parts would overflow.
    let cases = [
        (Num::MIN, Num::ZERO, -Num::FRAC_PI_2),
        (Num::ZERO, Num::MIN, Num::PI),
        (Num::MIN, Num::MIN, n(-39_530_384)),
        (Num::MAX, Num::MIN, n(39_530_384)),
        (Num::MIN, Num::MAX, n(-13_176_795)),
        (n(-1), Num::MIN, n(-52_707_179)),
    ];
    for (y, x, angle) in cases {
        assert_eq!(y.atan2(x), angle, "{y:?} {x:?}");
    }
    // The ends of the angles: sin and cos of MIN and MAX, exact to the nearest bit.
    let ends = [Num::MIN, Num::MAX].map(|angle| {
        let SinCos { sin, cos } = angle.sin_cos();
        (sin.to_bits(), cos.to_bits())
    });
    assert_eq!(ends, [(16_412_560, 3_478_915), (-16_412_560, 3_478_914)]);
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

/// Largest error in ulp of `sin_cos` over evenly spaced angles in `[from, to)`, each a whole
/// number of 2¹¹ bits, so that f64 holds it exactly up to 2⁶⁴.
fn sin_cos_max_ulp(from: i64, to: i64, count: i64) -> f64 {
    let mut worst = 0.0_f64;
    let span = (i128::from(to) - i128::from(from)) / i128::from(count);
    let step = i64::try_from(span).unwrap() & !((1 << 11) - 1);
    for i in 0..count {
        let angle = n(from + step * i);
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
    let worst = sin_cos_max_ulp(-8 * ONE, 8 * ONE, 200_000)
        .max(sin_cos_max_ulp(-(1 << 52), 1 << 52, 20_000))
        .max(sin_cos_max_ulp(-(1 << 62), 1 << 62, 20_000));
    assert!(worst <= SIN_COS_ULP, "sin_cos off by {worst} ulp");
}

#[test]
fn atan2_within_bound() {
    let worst = atan2_max_ulp(50 * ONE, 400).max(atan2_max_ulp(ONE / 64, 200));
    assert!(worst <= ATAN2_ULP, "atan2 off by {worst} ulp");
}

const SIN_COS_ULP: f64 = 0.501;
const ATAN2_ULP: f64 = 0.501;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn trig_symmetries(a in -(1_i64 << 45)..(1_i64 << 45), b in bits()) {
        let plus = n(a).sin_cos();
        let minus = (-n(a)).sin_cos();
        prop_assert_eq!(minus, SinCos { sin: -plus.sin, cos: plus.cos });
        prop_assert_eq!((-n(a)).atan2(n(b)), -n(a).atan2(n(b)));
    }

    #[test]
    fn a_product_by_a_quadruple_is_the_product(a in any::<i64>(), b in -(1_i64 << 61)..(1_i64 << 61)) {
        // (a · 4b) / 2⁶⁴ and (a · b) / 2⁶², each rounded down, are one number.
        prop_assert_eq!(mul_by_quadruple(a, b << 2), mul(a, b));
    }

    #[test]
    fn a_narrow_value_and_a_magnitude_round_as_a_wide_one(value in any::<i64>(), magnitude in 0..=(u64::MAX - (1 << 38))) {
        // Each to nearest, ties to even, from 2⁻⁶² to 2⁻²⁴, as the i128 shift gives it, over each
        // one's domain.
        let wide = |value: i128| Rounding::NearestEven.shift_right(value, WIDE_BITS - Num::FRAC_BITS);
        prop_assert_eq!(i128::from(to_num_narrow(value).to_bits()), wide(i128::from(value)));
        prop_assert_eq!(i128::from(to_num_magnitude(magnitude)), wide(i128::from(magnitude)));
    }
}
