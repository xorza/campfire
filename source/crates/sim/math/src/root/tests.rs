use proptest::prelude::*;

use super::*;

/// The cases of each property a run, as the other tests of the crate draw.
const CASES: u32 = 10_000;

#[test]
fn the_root_is_exact_at_each_edge_of_its_paths() {
    // (value, root): the first squares; the bounds of the paths, 2⁵³ where the float stops
    // holding every value, 2⁶⁴ where the path turns to `u128`, 2¹⁰² where the guess turns to a
    // residual step, 2¹⁰⁴, and 2¹²⁶, with one below and one above each. 2⁵³ =
    // 9,007,199,254,740,992 lies between 94,906,265² = 9,007,199,136,250,225 and 94,906,266² =
    // 9,007,199,326,062,756. The float root of 2¹⁰² − 1 is 2⁵¹, one past its floor, which the
    // step down corrects.
    let cases: [(u128, u128); 20] = [
        (0, 0),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 2),
        ((1 << 53) - 1, 94_906_265),
        (1 << 53, 94_906_265),
        ((1 << 53) + 1, 94_906_265),
        ((1 << 64) - 1, (1 << 32) - 1),
        (1 << 64, 1 << 32),
        ((1 << 64) + 1, 1 << 32),
        ((1 << 102) - 1, (1 << 51) - 1),
        (1 << 102, 1 << 51),
        ((1 << 102) + 1, 1 << 51),
        ((1 << 104) - 1, (1 << 52) - 1),
        (1 << 104, 1 << 52),
        ((1 << 104) + 1, 1 << 52),
        ((1 << 126) - 1, (1 << 63) - 1),
        (1 << 126, 1 << 63),
        ((1 << 126) + 1, 1 << 63),
    ];
    for (value, root) in cases {
        assert_eq!(value.floor_root(), root, "{value}");
    }
}

#[test]
fn the_root_is_exact_around_each_square() {
    // k² − 1 → k − 1, k² → k, and k² + 2k = (k + 1)² − 1 → k. The last k, 2⁶⁴ − 1, gives
    // k² + 2k = 2¹²⁸ − 1, `u128::MAX`.
    let roots: [u128; 7] = [
        (1 << 32) - 1,
        1 << 32,
        (1 << 52) - 1,
        1 << 52,
        (1 << 52) + 1,
        1 << 63,
        (1 << 64) - 1,
    ];
    for k in roots {
        let square = k * k;
        assert_eq!((square - 1).floor_root(), k - 1, "{k}² − 1");
        assert_eq!(square.floor_root(), k, "{k}²");
        assert_eq!((square + 2 * k).floor_root(), k, "{k}² + 2·{k}");
    }
    assert_eq!(u128::MAX.floor_root(), (1 << 64) - 1);
}

#[test]
fn the_nearest_root_turns_between_the_squares_of_halves() {
    // (n − ½)² = n² − n + ¼ and (n + ½)² = n² + n + ¼, so n² − n rounds to n − 1, n² − n + 1 and
    // n² + n to n, and n² + n + 1 to n + 1, for n on each side of each path's bound, up to
    // 2⁶³ − 1, whose n² + n + 1 is below 2¹²⁶. 0 is 0, 2 is √2 to 1, 3 is √3 to 2.
    assert_eq!(
        [0_u128, 1, 2, 3].map(RoundedRoot::nearest_root),
        [0, 1, 1, 2]
    );
    let roots: [u128; 9] = [
        2,
        (1 << 32) - 1,
        1 << 32,
        (1 << 50) + 1,
        (1 << 51) - 1,
        1 << 51,
        (1 << 51) + 1,
        1 << 62,
        (1 << 63) - 1,
    ];
    for n in roots {
        let square = n * n;
        assert_eq!((square - n).nearest_root(), n - 1, "{n}² − {n}");
        assert_eq!((square - n + 1).nearest_root(), n, "{n}² − {n} + 1");
        assert_eq!((square + n).nearest_root(), n, "{n}² + {n}");
        assert_eq!((square + n + 1).nearest_root(), n + 1, "{n}² + {n} + 1");
        // The order against `n`, with no root, turns at the same values, up to 2⁶³.
        for value in [square - n, square - n + 1, square + n, square + n + 1] {
            assert_eq!(
                value.nearest_root_cmp(n),
                value.nearest_root().cmp(&n),
                "{value} {n}"
            );
        }
    }
    // Against 0, only 0 is equal; at 2⁶³, the largest `other`, the order still turns there.
    assert_eq!(
        [0_u128, 1].map(|value| value.nearest_root_cmp(0)),
        [Ordering::Equal, Ordering::Greater]
    );
    let top = 1_u128 << 63;
    assert_eq!((top * top - top).nearest_root_cmp(top), Ordering::Less);
    assert_eq!((top * top + top).nearest_root_cmp(top), Ordering::Equal);
    assert_eq!(
        (top * top + top + 1).nearest_root_cmp(top),
        Ordering::Greater
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn the_root_is_isqrts_at_every_width(bits in any::<u128>(), shift in 0_u32..128) {
        let value = bits >> shift;
        prop_assert_eq!(value.floor_root(), value.isqrt());
    }

    #[test]
    fn the_root_is_exact_around_squares_of_every_width(bits in any::<u64>(), shift in 0_u32..64) {
        // k² − 1, k² and k² + 2k, where a float root lands on an integer or just short of one.
        let k = u128::from((bits >> shift).max(1));
        let square = k * k;
        prop_assert_eq!((square - 1).floor_root(), k - 1);
        prop_assert_eq!(square.floor_root(), k);
        prop_assert_eq!((square + 2 * k).floor_root(), k);
    }

    #[test]
    fn the_nearest_root_rounds_the_root(bits in any::<u128>(), shift in 2_u32..128) {
        let value = bits >> shift;
        let floor = value.isqrt();
        let nearest = if value - floor * floor > floor { floor + 1 } else { floor };
        prop_assert_eq!(value.nearest_root(), nearest);
        // Against a root on either side of it, or itself, the order is the nearest root's.
        for other in [nearest.saturating_sub(1), nearest, nearest + 1] {
            prop_assert_eq!(value.nearest_root_cmp(other), nearest.cmp(&other));
        }
    }
}

#[test]
fn the_ceiling_root_rounds_up_to_the_least_whole_root() {
    // 0, 1 and 4 are squares; 2, 3 and 5 round up to 2, 2 and 3.
    let small = [0_u128, 1, 2, 3, 4, 5].map(RoundedRoot::ceil_root);
    assert_eq!(small, [0, 1, 2, 2, 2, 3]);
    // Around each square k²: k² − 1 rounds up to k, k² is k, k² + 1 is k + 1.
    for k in [2_u128, 1 << 20, (1 << 32) + 7, (1 << 64) - 1] {
        let square = k * k;
        assert_eq!((square - 1).ceil_root(), k, "{k}² − 1");
        assert_eq!(square.ceil_root(), k, "{k}²");
        assert_eq!((square + 1).ceil_root(), k + 1, "{k}² + 1");
    }
    // The largest value's root, 2⁶⁴ − 1 and a bit, rounds up to 2⁶⁴.
    assert_eq!(u128::MAX.ceil_root(), 1 << 64);
}
