use proptest::prelude::*;

use super::*;

/// The cases of each property a run, as the other tests of the crate draw.
const CASES: u32 = 10_000;

/// The widest lanes, both signs, and the lanes around 0.
const EDGES: [i64; 8] = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX, 7];

/// Every lane of `op` on lanes against `scalar` on each, for each pair of `EDGES` in turn.
fn each_pair(op: impl Fn(I64x4, I64x4) -> I64x4, scalar: impl Fn(i64, i64) -> i64) {
    for &a in EDGES.as_chunks::<4>().0 {
        for &b in EDGES.as_chunks::<4>().0 {
            for turn in 0..4 {
                let mut b = b;
                b.rotate_left(turn);
                let got = op(I64x4(a), I64x4(b)).0;
                let expected = [0, 1, 2, 3].map(|lane| scalar(a[lane], b[lane]));
                assert_eq!(got, expected, "{a:?} {b:?}");
            }
        }
    }
}

#[test]
fn the_lane_ops_are_their_scalar_ops_at_the_edges() {
    each_pair(I64x4::wrapping_add, i64::wrapping_add);
    each_pair(I64x4::wrapping_sub, i64::wrapping_sub);
    each_pair(I64x4::max, i64::max);
    each_pair(I64x4::min, i64::min);
    let le = |a: I64x4, b: I64x4| {
        let bits = a.simd_le(b).to_bits();
        I64x4([0, 1, 2, 3].map(|lane| i64::from(bits >> lane & 1)))
    };
    each_pair(le, |a, b| i64::from(a <= b));
    assert_eq!(I64x4::splat(-5).0, [-5; 4]);
    assert_eq!(
        I64x4([-1, 0, 1, i64::MIN]).cast_unsigned().to_array(),
        [u64::MAX, 0, 1, 1 << 63]
    );
}

#[test]
fn narrow_products_are_exact_at_the_edges_of_i32() {
    // i32::MIN² = 2⁶², the largest; i32::MIN · i32::MAX = −2⁶² + 2³¹, the most negative.
    let a = I64x4([i32::MIN.into(), i32::MIN.into(), i32::MAX.into(), -1]);
    let b = I64x4([i32::MIN.into(), i32::MAX.into(), i32::MAX.into(), 0]);
    assert_eq!(
        a.mul_narrow(b).0,
        [
            1 << 62,
            -(1 << 62) + (1 << 31),
            (1 << 62) - (1 << 32) + 1,
            0
        ]
    );
}

#[test]
fn division_rounds_down_exactly_at_the_edges_of_its_domain() {
    // ±(2⁵¹ − 1) over 1, 2, 3 and 2⁵¹ − 1; around multiples of a cell of 2²⁴, both signs;
    // −1 over anything is −1, and 0 is 0.
    let edge = (1_i64 << 51) - 1;
    let cell = 1 << 24;
    let numerators = [
        [edge, -edge, 0, -1],
        [3 * cell - 1, 3 * cell, 3 * cell + 1, -3 * cell - 1],
        [-3 * cell, -3 * cell + 1, edge - 1, -(edge - 1)],
    ];
    for divisor in [1, 2, 3, cell, 2 * cell, edge] {
        for lanes in numerators {
            let expected = lanes.map(|lane| lane.div_euclid(divisor));
            assert_eq!(
                I64x4(lanes).div_euclid(divisor).0,
                expected,
                "{lanes:?} / {divisor}"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn division_is_div_euclid_over_its_domain(
        lanes in prop::array::uniform4(-(1_i64 << 51) + 1..1 << 51),
        divisor in 1_i64..1 << 51,
        shift in 0_u32..51,
    ) {
        let divisor = (divisor >> shift).max(1);
        let expected = lanes.map(|lane| lane.div_euclid(divisor));
        prop_assert_eq!(I64x4(lanes).div_euclid(divisor).0, expected);
    }
}
