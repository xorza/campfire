use proptest::prelude::*;

use super::*;

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

    // (3, 4, 0) m is 25 m² = 25 · 2⁴⁸ raw; one raw unit more on x adds 2 · 3 · 2²⁴ + 1.
    assert_eq!(v(3, 4, 0).length_squared_bits(), 25 << 48);
    assert_eq!(
        raw(3 << 24 | 1, 4 << 24, 0).length_squared_bits(),
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
    let encoded = postcard::to_allocvec(&a).unwrap();
    assert_eq!(
        encoded,
        postcard::to_allocvec(&(1_i64, -2_i64, i64::MIN)).unwrap()
    );
    assert_eq!(postcard::from_bytes::<Vec3>(&encoded).unwrap(), a);
}

fn component() -> impl Strategy<Value = i64> {
    prop_oneof![any::<i64>(), -(1_i64 << 45)..(1_i64 << 45), -64_i64..64,]
}

fn vector() -> impl Strategy<Value = Vec3> {
    (component(), component(), component()).prop_map(|(x, y, z)| raw(x, y, z))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

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
    fn normalized_matches_division(a in vector()) {
        let expected = a
            .checked_length()
            .filter(|&length| length != Num::ZERO)
            .and_then(|length| a.checked_div(length));
        prop_assert_eq!(a.normalized(), expected);
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
