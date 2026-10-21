use std::cmp::Ordering;

use super::*;

/// The scale of light: 1 is `2¹⁶`.
const SCALE: u32 = 1 << 16;

/// The denominator of the curve's base, `269025 / 25`.
const BASE: u128 = 10_761;

/// The product of three factors, exact, in base-2³² digits, the least significant first: the
/// test's own reference past 128 bits (07-determinism-core.md, Wider exact references stay in
/// tests), at most 384 bits, so 12 digits.
fn product(factors: [u128; 3]) -> [u64; 12] {
    let mut digits = [0_u64; 12];
    digits[0] = 1;
    for factor in factors {
        let factor =
            [0, 1, 2, 3].map(|at| u64::try_from((factor >> (32 * at)) & 0xFFFF_FFFF).unwrap());
        let mut out = [0_u64; 12];
        for (i, &x) in digits.iter().enumerate().filter(|&(_, &x)| x != 0) {
            let mut carry = 0;
            for (j, &y) in factor.iter().enumerate() {
                let sum = x * y + out[i + j] + carry;
                out[i + j] = sum & 0xFFFF_FFFF;
                carry = sum >> 32;
            }
            out[i + 4] += carry;
        }
        digits = out;
    }
    digits
}

/// Whether `light` is `SCALE · (n / BASE)^(12/5)` rounded to nearest: `(2L − 1)⁵ · BASE¹² ≤
/// (2 · SCALE)⁵ · n¹² < (2L + 1)⁵ · BASE¹²`, each side under 2²⁴⁷.
fn on_curve(light: u32, n: u128) -> bool {
    let side = |odd: u128| product([odd.pow(5), BASE.pow(6), BASE.pow(6)]);
    let target = product([(2 * u128::from(SCALE)).pow(5), n.pow(6), n.pow(6)]);
    let (low, high) = (
        side(2 * u128::from(light) - 1),
        side(2 * u128::from(light) + 1),
    );
    let order = |a: [u64; 12], b: [u64; 12]| a.iter().rev().cmp(b.iter().rev());
    order(low, target) != Ordering::Greater && order(target, high) == Ordering::Less
}

/// Whether `light` is `SCALE · num / den` rounded to nearest; `den` is never twice a tie.
fn on_line(light: u32, num: u128, den: u128) -> bool {
    let target = 2 * u128::from(SCALE) * num;
    (2 * u128::from(light) - 1) * den <= target && target < (2 * u128::from(light) + 1) * den
}

#[test]
fn each_entry_is_the_curve_rounded_to_nearest() {
    assert_eq!((LIGHT[0], LIGHT[255]), (0, SCALE));
    // Codes up to 10 lie on the line, 10/255 ≤ 0.04045; 12.92 · 255 = 3294.6.
    for (code, &light) in (0_u128..).zip(&LIGHT).take(255).skip(1) {
        let rounded = if code <= 10 {
            on_line(light, 100 * code, 329_460)
        } else {
            on_curve(light, 40 * code + 561)
        };
        assert!(rounded, "code {code}");
    }
    // Midpoints up to 9.5/255 lie on the line, the rest on the curve, at 40 (k + ½) + 561.
    for (k, &midpoint) in (0_u128..).zip(&MIDPOINTS) {
        let rounded = if k <= 9 {
            on_line(midpoint, 100 * (2 * k + 1), 658_920)
        } else {
            on_curve(midpoint, 40 * k + 581)
        };
        assert!(rounded, "midpoint {k}");
    }
    assert!((0..255).all(|k| LIGHT[k] < MIDPOINTS[k] && MIDPOINTS[k] < LIGHT[k + 1]));
}

#[test]
fn a_mean_of_light_encodes_to_its_nearest_code() {
    // Each code alone encodes to itself.
    assert!((0..=255).all(|code| encode(u64::from(light(code)), 1) == code));
    // Black and white in equal parts are light ½, which sRGB encodes as 0.73536 · 255 =
    // 187.52, so 188, not the 128 a mean of the codes gives; one part black to two of white
    // is light ⅔, 0.83601 · 255 = 213.18, so 213.
    assert_eq!(encode(2 * u64::from(SCALE), 4), 188);
    assert_eq!(encode(2 * u64::from(SCALE), 3), 213);
}
