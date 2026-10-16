use campfire_codec::Binary;
use proptest::prelude::*;

use super::*;

/// The cases of each property a run, of new random inputs each run, as proptest draws by
/// default: a failure saves its seed under `proptest-regressions/`, which goes into version
/// control, so every later run tries it first.
const CASES: u32 = 10_000;

const ONE: i64 = 1 << 24;
const HALF: i64 = 1 << 23;

const fn n(bits: i64) -> Num {
    Num::from_bits(bits)
}

fn v(x: i64, y: i64, z: i64) -> Vec3 {
    Vec3::new(
        Num::from_int(x).unwrap(),
        Num::from_int(y).unwrap(),
        Num::from_int(z).unwrap(),
    )
}

const fn raw(x: i64, y: i64, z: i64) -> Vec3 {
    Vec3::new(n(x), n(y), n(z))
}

/// The integer nearest to √value, by `core`'s exact integer root.
fn nearest_root(value: u128) -> u128 {
    let floor = value.isqrt();
    if value - floor * floor > floor {
        floor + 1
    } else {
        floor
    }
}

fn squared_bits(a: Vec3) -> u128 {
    [a.x, a.y, a.z]
        .iter()
        .map(|c| u128::from(c.to_bits().unsigned_abs()).pow(2))
        .sum()
}

#[test]
fn arithmetic() {
    assert_eq!(v(1, 2, 3) + v(4, -5, 6), v(5, -3, 9));
    assert_eq!(v(1, 2, 3) - v(4, -5, 6), v(-3, 7, -3));
    assert_eq!(-v(1, -2, 3), v(-1, 2, -3));
    // (1, 2, 3) · 1.5 = (1.5, 3, 4.5); (3, 4, 5) / 2 = (1.5, 2, 2.5).
    assert_eq!(
        v(1, 2, 3) * n(ONE + HALF),
        raw(ONE + HALF, 3 * ONE, 4 * ONE + HALF)
    );
    assert_eq!(
        v(3, 4, 5) / n(2 * ONE),
        raw(ONE + HALF, 2 * ONE, 2 * ONE + HALF)
    );
    let max = raw(i64::MAX, 0, 0);
    assert_eq!(max.checked_add(raw(1, 0, 0)), None);
    assert_eq!(raw(0, 0, i64::MIN).checked_neg(), None);
    assert_eq!(v(1, 1, 1).checked_div(Num::ZERO), None);
    // Each assigning operator gives what its operator does: (1, 2, 3) + (4, −5, 6) − (1, 1, 1)
    // = (4, −4, 8), · 1.5 = (6, −6, 12), / 2 = (3, −3, 6).
    let mut at = v(1, 2, 3);
    at += v(4, -5, 6);
    at -= v(1, 1, 1);
    assert_eq!(at, v(4, -4, 8));
    at *= n(ONE + HALF);
    assert_eq!(at, v(6, -6, 12));
    at /= n(2 * ONE);
    assert_eq!(at, v(3, -3, 6));
}

#[test]
fn dot_rounds_once() {
    assert_eq!(v(1, 2, 3).dot(v(4, -5, 6)), Num::from_int(12).unwrap());
    // ε · 0.5 + ε · 0.5 = ε exactly; rounding each product first would give 0 + 0.
    assert_eq!(raw(1, 1, 0).dot(raw(HALF, HALF, 0)), Num::EPSILON);
    assert_eq!(
        raw(i64::MAX, i64::MAX, i64::MAX).checked_dot(raw(i64::MAX, i64::MAX, i64::MAX)),
        None
    );
}

#[test]
fn length_and_distance() {
    assert_eq!(v(3, 4, 0).length(), Num::from_int(5).unwrap());
    assert_eq!(v(2, 3, 6).length(), Num::from_int(7).unwrap());
    assert_eq!(Vec3::ZERO.length(), Num::ZERO);
    // 3 · 2⁴⁸ is the exact squared length in raw units; √3 · 2²⁴ = 29 058 990.52… → 29 058 991.
    assert_eq!(v(1, 1, 1).length(), n(29_058_991));
    assert_eq!(v(1, 2, 3).distance(v(4, 6, 3)), Num::from_int(5).unwrap());
    // Three components of 2⁶³ square to 3 · 2¹²⁶: its root exceeds Num.
    assert_eq!(raw(i64::MIN, i64::MIN, i64::MIN).checked_length(), None);
    assert_eq!(
        raw(i64::MAX, 0, 0).checked_distance(raw(i64::MIN, 0, 0)),
        None
    );
}

#[test]
fn within_is_exact_at_the_boundary() {
    let origin = Vec3::ZERO;
    let five = Num::from_int(5).unwrap();
    assert!(origin.within(v(3, 4, 0), five));
    assert!(!origin.within(v(3, 4, 0), five - Num::EPSILON));
    assert!(origin.within(origin, Num::ZERO));
    assert!(!origin.within(origin, -Num::EPSILON));
    assert!(!raw(i64::MAX, 0, 0).within(raw(i64::MIN, 0, 0), Num::MAX));
    // On the ground plane the heights drop: (1, 0, 1) to (4, 9, 5) is 3 m along x and 4 m along
    // z, 5 m, whatever the heights.
    let (from, to) = (v(1, 0, 1), v(4, 9, 5));
    assert_eq!(to.ground(), v(4, 0, 5));
    assert_eq!(from.ground_offset(to), v(3, 0, 4));
    let reaches = [five - Num::EPSILON, five, five + Num::EPSILON]
        .map(|radius| from.within_ground(to, radius));
    assert_eq!(reaches, [false, true, true]);

    // (3, 4, 0) m is 25 m² = 25 · 2⁴⁸ raw; one raw unit more on x adds 2 · 3 · 2²⁴ + 1.
    assert_eq!(v(3, 4, 0).length_squared_bits(), 25 << 48);
    assert_eq!(
        raw((3 << 24) | 1, 4 << 24, 0).length_squared_bits(),
        (25 << 48) + (6 << 24) + 1
    );
    // Three components of −2⁶³ square to 3 · 2¹²⁶, still within a `u128`.
    assert_eq!(
        raw(i64::MIN, i64::MIN, i64::MIN).length_squared_bits(),
        3 << 126
    );
}

#[test]
fn normalized_rounds_each_component() {
    // 3/5 · 2²⁴ = 10 066 329.6 → 10 066 330; 4/5 · 2²⁴ = 13 421 772.8 → 13 421 773.
    assert_eq!(
        v(3, 4, 0).normalized(),
        Some(raw(10_066_330, 13_421_773, 0))
    );
    // As short a vector as one bit each way is as exact: (1, 1) bits is 2²⁴ / √2 =
    // 11 863 283.2 bits each way, where a length rounded to a bit first gave 2²⁴.
    assert_eq!(
        raw(1, 1, 0).normalized(),
        Some(raw(11_863_283, 11_863_283, 0))
    );
    assert_eq!(
        raw(-1, 0, 1).normalized(),
        Some(raw(-11_863_283, 0, 11_863_283))
    );
    // Past ±2⁴⁷ bits a component is no offset of two places, and has no unit.
    assert_eq!(raw(1 << 47, 0, 0).normalized(), None);
    assert_eq!(raw((1 << 47) - 1, 0, 0).normalized(), Some(v(1, 0, 0)));
    assert_eq!(v(0, 0, -7).normalized(), Some(v(0, 0, -1)));
    assert_eq!(Vec3::ZERO.normalized(), None);
    assert_eq!(v(1, 1, 1).direction_to(v(1, 1, 3)), Some(v(0, 0, 1)));
    assert_eq!(v(1, 1, 1).direction_to(v(1, 1, 1)), None);
}

#[test]
fn step_toward_rounds_once_and_stops_at_the_target() {
    let start = v(1, 2, 3);
    // Offset (3, 0, 4), distance 5: one meter moves 3/5 and 4/5, as in `normalized`.
    assert_eq!(
        start.step_toward(v(4, 2, 7), Num::ONE),
        raw(ONE + 10_066_330, 2 * ONE, 3 * ONE + 13_421_773)
    );
    // Two meters: 6/5 · 2²⁴ = 20 132 659.2 → 20 132 659; 8/5 · 2²⁴ = 26 843 545.6 → 26 843 546.
    assert_eq!(
        start.step_toward(v(4, 2, 7), n(2 * ONE)),
        raw(ONE + 20_132_659, 2 * ONE, 3 * ONE + 26_843_546)
    );
    assert_eq!(start.step_toward(v(4, 2, 7), n(5 * ONE)), v(4, 2, 7));
    assert_eq!(start.step_toward(v(4, 2, 7), n(9 * ONE)), v(4, 2, 7));
    assert_eq!(start.step_toward(v(4, 2, 7), Num::ZERO), start);
    assert_eq!(start.step_toward(start, Num::ZERO), start);
    assert_eq!(
        raw(i64::MIN, 0, 0).checked_step_toward(raw(i64::MAX, 0, 0), n(HALF)),
        None
    );
}

#[test]
fn rotated_y_turns_on_the_ground_plane() {
    let quarter = Num::FRAC_PI_2.sin_cos();
    let half = Num::PI.sin_cos();
    // (x, z) → (x cos + z sin, z cos − x sin): a quarter turn sends +x to −z; y never changes.
    assert_eq!(v(1, 7, 0).rotated_y(quarter), v(0, 7, -1));
    assert_eq!(v(0, 7, 1).rotated_y(quarter), v(1, 7, 0));
    assert_eq!(v(2, -3, 5).rotated_y(half), v(-2, -3, -5));
    assert_eq!(v(2, -3, 5).rotated_y(Num::ZERO.sin_cos()), v(2, -3, 5));
}

#[test]
fn serializes_as_three_nums() {
    let a = raw(1, -2, i64::MIN);
    let encoded = Binary::encode(&a);
    assert_eq!(encoded, Binary::encode(&(1_i64, -2_i64, i64::MIN)));
    assert_eq!(Binary::decode::<Vec3>(&encoded).unwrap(), a);
}

/// The true `component · 2²⁴ / √square` by floats, within 2⁻²⁶ of it, rounded to nearest, where
/// it lies past 2⁻²⁰ of a half, so the float's error cannot cross it.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::float_arithmetic,
    reason = "a float reference within 2⁻²⁶ of the true quotient"
)]
fn true_unit(component: i64, square: u128) -> Option<i64> {
    let exact = component as f64 * 16_777_216.0 / (square as f64).sqrt();
    let rest = exact - exact.floor();
    ((rest - 0.5).abs() > 1.0 / 1_048_576.0).then(|| exact.round() as i64)
}

fn component() -> impl Strategy<Value = i64> {
    prop_oneof![any::<i64>(), -(1_i64 << 45)..(1_i64 << 45), -64_i64..64,]
}

fn vector() -> impl Strategy<Value = Vec3> {
    (component(), component(), component()).prop_map(|(x, y, z)| raw(x, y, z))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn length_is_the_nearest_root(a in vector()) {
        let expected = i64::try_from(nearest_root(squared_bits(a))).ok().map(Num::from_bits);
        prop_assert_eq!(a.checked_length(), expected);
    }

    #[test]
    fn within_matches_exact_squares(a in vector(), b in vector(), r in 0_i64..(1_i64 << 50)) {
        let expected = b.checked_sub(a).is_some_and(|d| squared_bits(d) <= u128::from(r.cast_unsigned()).pow(2));
        prop_assert_eq!(a.within(b, n(r)), expected);
    }

    #[test]
    fn normalized_is_each_true_component_rounded_once(a in vector()) {
        // The true quotient `c · 2²⁴ / √square` by floats, within 2⁻²⁶ of it, rounds as the
        // component must wherever it lies past 2⁻²⁰ of a half.
        let components = [a.x, a.y, a.z].map(Num::to_bits);
        let wide = components.iter().any(|&component| component.unsigned_abs() >= 1 << 47);
        let Some(unit) = a.normalized() else {
            prop_assert!(wide || a == Vec3::ZERO);
            return Ok(());
        };
        prop_assert!(!wide);
        for (component, got) in components.into_iter().zip([unit.x, unit.y, unit.z]) {
            if let Some(rounded) = true_unit(component, squared_bits(a)) {
                prop_assert_eq!(got.to_bits(), rounded);
            }
        }
    }

    #[test]
    fn step_toward_never_passes_the_target(a in vector(), b in vector(), step in 0_i64..(1_i64 << 50)) {
        if let Some(moved) = a.checked_step_toward(b, n(step)) {
            for (from, to, at) in [(a.x, b.x, moved.x), (a.y, b.y, moved.y), (a.z, b.z, moved.z)] {
                prop_assert!(from.min(to) <= at && at <= from.max(to));
            }
            prop_assert_eq!(moved == b, a.within(b, n(step)));
        }
    }

    #[test]
    fn dot_is_symmetric(a in vector(), b in vector()) {
        prop_assert_eq!(a.checked_dot(b), b.checked_dot(a));
    }
}

#[test]
fn a_triple_converts_each_element_exactly() {
    // A tuple's elements may differ in type; an array's are one.
    let mixed = Vec3::from((-1_i8, Num::HALF, u32::MAX));
    assert_eq!(
        mixed,
        Vec3::new(Num::int(-1), Num::HALF, Num::int(4_294_967_295))
    );
    let wide = Vec3::from([i32::MIN, 0, i32::MAX]);
    assert_eq!(
        wide,
        Vec3::new(Num::int(-2_147_483_648), Num::ZERO, Num::int(2_147_483_647))
    );
    assert_eq!(Vec3::from((3, 0, 4)), v(3, 0, 4));
}

#[test]
fn a_unit_component_near_a_half_rounds_by_the_exact_half() {
    // (1, 1) bits: each component is 2²⁴ / √2 = 11 863 283.203 bits, below the half 11 863 283.5,
    // as 4 · 2⁴⁸ = 2⁵⁰ is below 23 726 567² · 2. A unit of 2⁻⁶² at that half, or a unit either
    // side of it, gives the true rounding, the floor; a unit 3 past the half gives its own.
    let floor = 11_863_283_i64 << 38;
    let half = floor + (1 << 37);
    assert!(4_u128 << 48 < 23_726_567_u128.pow(2) * 2);
    for unit in [half - 2, half, half + 2] {
        assert_eq!(
            unit_component(unit, 1, 2),
            Num::from_bits(11_863_283),
            "{unit}"
        );
        assert_eq!(
            unit_component(-unit, -1, 2),
            Num::from_bits(-11_863_283),
            "{unit}"
        );
    }
    assert_eq!(unit_component(half + 3, 1, 2), Num::from_bits(11_863_284));
    // The wide product of two 128-bit factors, high and low halves.
    assert_eq!(
        WideProduct::of(u128::MAX, u128::MAX),
        WideProduct {
            high: u128::MAX - 1,
            low: 1,
        }
    );
    assert_eq!(
        WideProduct::of(1 << 100, 1 << 100),
        WideProduct {
            high: 1 << 72,
            low: 0,
        }
    );
    assert_eq!(WideProduct::of(3, 5), WideProduct { high: 0, low: 15 });
    // The high half orders first: 2¹²⁸ is past every product below it, 2¹²⁸ − 1 among them.
    assert!(WideProduct::of(1 << 64, 1 << 64) > WideProduct::of(u128::MAX, 1));
}
