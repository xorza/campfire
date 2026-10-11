use proptest::prelude::*;

use super::*;

/// The cases of each property a run, as the other tests of the crate draw.
const CASES: u32 = 10_000;

/// Both ways against the division, for one step and its offsets.
fn check(step: i64, distance: i64, offsets: [i64; 3]) {
    let moves = StepMoves::new(step, distance);
    let offsets = offsets.map(Num::from_bits);
    let expected = offsets.map(|offset| moves.divided(offset));
    assert_eq!(
        moves.of(offsets),
        expected,
        "{step} / {distance} of {offsets:?}"
    );
    assert_eq!(
        aarch64::moves(moves, offsets),
        expected,
        "{step} / {distance} of {offsets:?}"
    );
}

#[test]
fn each_way_rounds_each_move_as_the_division() {
    // 3 · 1 / 2 = 1.5 ties to 2, 5 · 1 / 2 = 2.5 to 2, and their negatives to −2 and −2; 7 · 2 / 3
    // = 4.67 to 5; a whole step moves the whole offset; the distances either side of 2⁵⁰, where
    // ARM turns to the division, and the largest; and a step of 0.
    check(1, 2, [3, 5, -3]);
    check(1, 2, [-5, 0, 1]);
    check(2, 3, [7, -7, 3]);
    check(1 << 40, 1 << 40, [1 << 40, -(1 << 39), 12_345]);
    for distance in [(1 << 50) - 1, 1 << 50, i64::MAX] {
        check(distance / 3, distance, [distance, -distance, distance / 7]);
    }
    check(0, 100, [100, -100, 1]);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn each_way_is_the_division(distance in 1_i64.., step in any::<u64>(), offsets in any::<[i64; 3]>(), shift in 0_u32..63) {
        // Distances of every width, and offsets within them, as a vector's components are.
        let distance = (distance >> shift).max(1);
        let step = i64::try_from(step % (distance.cast_unsigned() + 1)).unwrap();
        check(step, distance, offsets.map(|offset| offset % (distance + 1)));
    }
}
