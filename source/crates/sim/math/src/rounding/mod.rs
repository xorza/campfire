use crate::wide_division::WideDivision;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// How an exact quotient that is not whole becomes a whole one: the caller names it, as Java's
/// `RoundingMode` and IEEE 754's rounding attributes do, so no caller rounds on raw bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    /// To the nearest, a tie to the even one: the rounding of `Num`'s operators.
    NearestEven,
    /// Down, toward −∞.
    Floor,
    /// Up, toward +∞.
    Ceiling,
}

impl Rounding {
    /// `numerator / denominator`, rounded. The denominator is not 0, and neither is `i128::MIN`,
    /// whose magnitude overflows. One unsigned division of the magnitudes gives the quotient
    /// toward 0 and its rest, where a floor and a rest of signed values take two; a negative
    /// quotient rounds its magnitude the mirrored way. A `u128` division is a library call:
    /// magnitudes that fit 64 bits take the native division, 3.4 times as fast on an M2, and
    /// others `WideDivision`, each platform's fastest.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the 64-bit division takes only magnitudes below 2⁶⁴"
    )]
    pub fn divide(self, numerator: i128, denominator: i128) -> i128 {
        debug_assert!(denominator != 0, "a division by 0");
        debug_assert!(numerator != i128::MIN && denominator != i128::MIN);
        // The sign first, so only it lives across the division, which may be a call.
        let negative = (numerator < 0) != (denominator < 0);
        let (magnitude, divisor) = (numerator.unsigned_abs(), denominator.unsigned_abs());
        let (toward_zero, rest) = if (magnitude | divisor) >> 64 == 0 {
            let (magnitude, divisor) = (magnitude as u64, divisor as u64);
            (
                u128::from(magnitude / divisor),
                u128::from(magnitude % divisor),
            )
        } else {
            let WideDivision { quotient, rest } = WideDivision::of(magnitude, divisor);
            (quotient, rest)
        };
        self.round_quotient(toward_zero, rest, divisor, negative)
    }

    /// `divide` for constants: the same quotient, by the `u128` division alone, which a `const`
    /// function can call.
    pub(crate) const fn divide_in_const(self, numerator: i128, denominator: i128) -> i128 {
        debug_assert!(denominator != 0, "a division by 0");
        debug_assert!(numerator != i128::MIN && denominator != i128::MIN);
        let (magnitude, divisor) = (numerator.unsigned_abs(), denominator.unsigned_abs());
        self.round_quotient(
            magnitude / divisor,
            magnitude % divisor,
            divisor,
            (numerator < 0) != (denominator < 0),
        )
    }

    /// The quotient of magnitudes `toward_zero` with its rest over `divisor`, rounded, and
    /// negated when `negative`.
    const fn round_quotient(
        self,
        toward_zero: u128,
        rest: u128,
        divisor: u128,
        negative: bool,
    ) -> i128 {
        // Twice the rest fits, as the divisor is below 2¹²⁷. A rest takes the magnitude one past
        // `toward_zero` only for a divisor of 2 or more, so below 2¹²⁶: neither the sum nor its
        // negation can overflow, and the wrapping steps skip the checks a release build makes.
        // The tie test sits in the `if` it decides: computed into a value first, it compiles to
        // a select after the division, which measured 14 % slower.
        let twice = rest << 1;
        let away = match self {
            Self::NearestEven => twice > divisor || (twice == divisor && toward_zero & 1 == 1),
            Self::Floor => negative && rest != 0,
            Self::Ceiling => !negative && rest != 0,
        };
        let rounded = if away {
            toward_zero.wrapping_add(1)
        } else {
            toward_zero
        };
        let rounded = rounded.cast_signed();
        if negative {
            rounded.wrapping_neg()
        } else {
            rounded
        }
    }

    /// `value / 2^shift`, rounded, for a shift from 1 to 127.
    pub const fn shift_right(self, value: i128, shift: u32) -> i128 {
        debug_assert!(0 < shift && shift < 128);
        let floor = value >> shift;
        let mask = (1_u128 << shift).wrapping_sub(1);
        let rest = value.cast_unsigned() & mask;
        // The quotient rounds up when the rest passes a threshold: to nearest, half less the
        // floor's last bit, so a rest past half, or of half from an odd floor; up, 0; down, the
        // mask, which no rest passes. The floor is at most half the value, so one more fits, and
        // the wrapping step skips the check a release build makes.
        let threshold = match self {
            Self::NearestEven => (mask - (mask >> 1)) - (floor & 1).cast_unsigned(),
            Self::Floor => mask,
            Self::Ceiling => 0,
        };
        floor.wrapping_add((rest > threshold) as i128)
    }
}

#[cfg(test)]
mod tests {
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
}
