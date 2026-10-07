use proptest::prelude::*;

use super::*;

/// The cases of each property a run, as the other tests of the crate draw.
const CASES: u32 = 10_000;

#[test]
fn the_root_is_exact_at_each_edge_of_its_paths() {
    // (value, root): the first squares; the bounds of the paths, 2⁵³ where the float stops
    // holding every value, 2⁶⁴ where the path turns to `u128`, 2¹⁰⁴ where the Newton step
    // starts, and 2¹²⁶, with one below and one above each. 2⁵³ = 9,007,199,254,740,992 lies
    // between 94,906,265² = 9,007,199,136,250,225 and 94,906,266² = 9,007,199,326,062,756.
    let cases: [(u128, u128); 17] = [
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn the_root_is_cores_at_every_width(bits in any::<u128>(), shift in 0_u32..128) {
        let value = bits >> shift;
        prop_assert_eq!(value.floor_root(), value.isqrt());
    }
}
