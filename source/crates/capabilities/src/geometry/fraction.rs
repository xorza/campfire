use std::cmp::Ordering;

use campfire_math::{Num, Rounding, U256};

/// A rational number `num / den` with a positive denominator, compared exactly: a share of a
/// straight path, where a box first comes nearest it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fraction {
    num: i128,
    den: i128,
}

impl Fraction {
    pub(crate) const ZERO: Fraction = Fraction { num: 0, den: 1 };
    pub(crate) const ONE: Fraction = Fraction { num: 1, den: 1 };

    /// `num / den`; a negative denominator turns both signs.
    pub(crate) const fn new(num: i128, den: i128) -> Fraction {
        debug_assert!(den != 0, "a fraction's denominator is not 0");
        if den < 0 {
            Fraction {
                num: -num,
                den: -den,
            }
        } else {
            Fraction { num, den }
        }
    }
}

impl Fraction {
    /// The share of `value` the fraction gives, from 0 to 1 of it, rounded to the nearest bit,
    /// ties to even.
    pub(crate) fn of(self, value: Num) -> Num {
        debug_assert!(Fraction::ZERO <= self && self <= Fraction::ONE && value >= Num::ZERO);
        let bits = U256::product(
            value.to_bits().unsigned_abs().into(),
            self.num.unsigned_abs(),
        )
        .div_rounded(self.den.unsigned_abs(), Rounding::NearestEven)
        .expect("a share of a number fits it");
        Num::from_bits(i64::try_from(bits).expect("a share of a number fits it"))
    }
}

impl Ord for Fraction {
    /// By `num₁ × den₂` against `num₂ × den₁`, each magnitude's product exact in 256 bits.
    fn cmp(&self, other: &Fraction) -> Ordering {
        let (sign, other_sign) = (self.num.signum(), other.num.signum());
        if sign != other_sign {
            return sign.cmp(&other_sign);
        }
        let magnitude = U256::product(self.num.unsigned_abs(), other.den.unsigned_abs()).cmp(
            &U256::product(other.num.unsigned_abs(), self.den.unsigned_abs()),
        );
        if sign < 0 {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Fraction) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Equal in value, as `1/2` and `2/4` are.
impl PartialEq for Fraction {
    fn eq(&self, other: &Fraction) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Fraction {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_compare_by_value() {
        let f = Fraction::new;
        // 1/2 = 2/4 = −1/−2; 1/3 < 1/2; −1/2 < −1/3 < 0 < 1/3.
        assert_eq!(f(1, 2), f(2, 4));
        assert_eq!(f(1, 2), f(-1, -2));
        assert!(f(1, 3) < f(1, 2));
        assert!(f(-1, 2) < f(-1, 3));
        assert!(f(-1, 3) < Fraction::ZERO && Fraction::ZERO < f(1, 3));
        assert_eq!(f(0, 7), Fraction::ZERO);
        assert_eq!(f(5, 5), Fraction::ONE);
        // Past u128 in the products: (2¹²⁶ + 1) / 2¹²⁶ just above 1, and (2¹²⁶ − 1) / 2¹²⁶
        // just below; negated, the other way.
        let big = 1_i128 << 126;
        assert!(f(big + 1, big) > Fraction::ONE);
        assert!(f(big - 1, big) < Fraction::ONE);
        assert!(f(-(big + 1), big) < f(-(big - 1), big));
        // A share of a number: 2/5 of 10 m is 4 m; 1/3 of a bit rounds to none, 2/3 to one.
        assert_eq!(f(2, 5).of(Num::int(10)), Num::int(4));
        assert_eq!(f(1, 3).of(Num::EPSILON), Num::ZERO);
        assert_eq!(f(2, 3).of(Num::EPSILON), Num::EPSILON);
        assert_eq!(Fraction::ONE.of(Num::int(7)), Num::int(7));
    }
}
