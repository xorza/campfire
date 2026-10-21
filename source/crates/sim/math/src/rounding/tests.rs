use super::*;

const MODES: [Rounding; 3] = [Rounding::NearestEven, Rounding::Floor, Rounding::Ceiling];

#[test]
fn each_mode_rounds_each_side_and_each_tie() {
    // Each row: numerator, denominator, then nearest-even, floor and ceiling. 7/2 = 3.5 ties
    // to 4, 5/2 = 2.5 to 2, their negatives to −4 and −2; 7/3 = 2.33 and 8/3 = 2.67 lie
    // either side of the half; −7/3 = −2.33; 6/3 is whole; a negative denominator turns
    // both signs.
    let cases = [
        (7, 2, [4, 3, 4]),
        (5, 2, [2, 2, 3]),
        (-7, 2, [-4, -4, -3]),
        (-5, 2, [-2, -3, -2]),
        (7, 3, [2, 2, 3]),
        (8, 3, [3, 2, 3]),
        (-7, 3, [-2, -3, -2]),
        (6, 3, [2, 2, 2]),
        (7, -2, [-4, -4, -3]),
        (0, 5, [0, 0, 0]),
    ];
    for (numerator, denominator, expected) in cases {
        let got = MODES.map(|mode| mode.divide(numerator, denominator));
        assert_eq!(got, expected, "{numerator} / {denominator}");
    }
    // At the ends: (2¹²⁷ − 1) / 2 is 2¹²⁶ − ½, a tie to the even 2¹²⁶, and its negative to
    // −2¹²⁶; over −1 it is −(2¹²⁷ − 1) whole.
    let top = i128::MAX;
    assert_eq!(Rounding::NearestEven.divide(top, 2), 1 << 126);
    assert_eq!(Rounding::Floor.divide(-top, 2), -(1 << 126));
    assert_eq!(MODES.map(|mode| mode.divide(top, -1)), [-top; 3]);
    // Either side of 2⁶⁴, where magnitudes leave the 64-bit division: (2⁶⁴ − 1) / 2 is
    // 2⁶³ − ½, a tie to the even 2⁶³; 2⁶⁴ / 3 is 6148914691236517205 rest 1, as 2⁶⁴ ≡ 1
    // mod 3; (2⁶⁴ − 1) / 2⁶⁴ lies just below 1, and its negative just above −1;
    // (2⁶⁴ + 2⁶³) / 2⁶⁴ = 1.5 ties to 2; (2⁶⁴ − 1) / (2⁶⁴ − 1) is whole.
    let (below, at) = ((1_i128 << 64) - 1, 1_i128 << 64);
    let third = 6_148_914_691_236_517_205;
    let cases = [
        (below, 2, [1 << 63, (1 << 63) - 1, 1 << 63]),
        (at, 3, [third, third, third + 1]),
        (below, at, [1, 0, 1]),
        (-below, at, [-1, -1, 0]),
        (at + (1 << 63), at, [2, 1, 2]),
        (below, below, [1, 1, 1]),
    ];
    for (numerator, denominator, expected) in cases {
        let got = MODES.map(|mode| mode.divide(numerator, denominator));
        assert_eq!(got, expected, "{numerator} / {denominator}");
        let in_const = MODES.map(|mode| mode.divide_in_const(numerator, denominator));
        assert_eq!(in_const, expected, "{numerator} / {denominator} in const");
    }
}

#[test]
fn a_shift_rounds_as_its_division() {
    // 5 / 2 = 2.5 ties to 2, 7 / 2 = 3.5 to 4, −5 / 4 = −1.25; and the whole range of shifts
    // over values either side, each against the division by 2^shift.
    assert_eq!(MODES.map(|mode| mode.shift_right(5, 1)), [2, 2, 3]);
    assert_eq!(MODES.map(|mode| mode.shift_right(7, 1)), [4, 3, 4]);
    assert_eq!(MODES.map(|mode| mode.shift_right(-5, 2)), [-1, -2, -1]);
    for shift in 1..126 {
        for value in [1_i128, 3, -3, 1 << 100, -(1 << 100) - 1, (1 << 125) + 7] {
            for mode in MODES {
                let divided = mode.divide(value, 1 << shift);
                assert_eq!(
                    mode.shift_right(value, shift),
                    divided,
                    "{value} >> {shift}"
                );
            }
        }
    }
}
