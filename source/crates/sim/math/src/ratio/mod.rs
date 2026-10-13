use std::cmp::Ordering;

use crate::i256::I256;
use crate::num::Num;
use crate::rounding::Rounding;
use crate::u256::U256;
use crate::u384::U384;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// A rational number `num / den` with a positive denominator, compared exactly by its cross
/// products, in an integer that holds them, as `num-rational`'s `Ratio<T>` is generic over its
/// integer: a share of a straight path, `Ratio<i128, i128>`, or a squared distance on the ground
/// plane, a cross product's square over a squared length, `Ratio<U256, u128>`.
#[derive(Debug, Clone, Copy)]
pub struct Ratio<N, D> {
    num: N,
    den: D,
}

/// An integer of a ratio, its numerator or its denominator.
pub trait RatioPart: Copy + Ord {
    const ZERO: Self;
    const ONE: Self;
}

/// A numerator whose product with a denominator of `D` one integer holds exactly: the cross
/// products a ratio compares by.
pub trait CrossProduct<D>: RatioPart {
    /// The product, wide enough that none passes it.
    type Product: Ord;

    fn cross(self, den: D) -> Self::Product;
}

impl RatioPart for i128 {
    const ZERO: i128 = 0;
    const ONE: i128 = 1;
}

impl RatioPart for u128 {
    const ZERO: u128 = 0;
    const ONE: u128 = 1;
}

impl RatioPart for U256 {
    const ZERO: U256 = U256::ZERO;
    const ONE: U256 = U256::from_u128(1);
}

impl CrossProduct<i128> for i128 {
    type Product = I256;

    fn cross(self, den: i128) -> I256 {
        I256::product(self, den)
    }
}

impl CrossProduct<u128> for U256 {
    type Product = U384;

    fn cross(self, den: u128) -> U384 {
        U384::product(self, den)
    }
}

impl<N: RatioPart, D: RatioPart> Ratio<N, D> {
    pub const ZERO: Ratio<N, D> = Ratio {
        num: N::ZERO,
        den: D::ONE,
    };
    pub const ONE: Ratio<N, D> = Ratio {
        num: N::ONE,
        den: D::ONE,
    };

    /// `num / den`, `den` positive.
    pub fn new(num: N, den: D) -> Ratio<N, D> {
        debug_assert!(den > D::ZERO, "a ratio's denominator is positive");
        Ratio { num, den }
    }

    /// `num / 1`.
    pub const fn whole(num: N) -> Ratio<N, D> {
        Ratio { num, den: D::ONE }
    }
}

impl Ratio<i128, i128> {
    /// The share of `value` it gives, from 0 to 1 of it, rounded to the nearest bit, ties to
    /// even.
    pub fn of(self, value: Num) -> Num {
        debug_assert!(Ratio::ZERO <= self && self <= Ratio::ONE && value >= Num::ZERO);
        let bits = U256::product(
            value.to_bits().unsigned_abs().into(),
            self.num.unsigned_abs(),
        )
        .div_rounded(self.den.unsigned_abs(), Rounding::NearestEven)
        .expect("a share of a number fits it");
        Num::from_bits(i64::try_from(bits).expect("a share of a number fits it"))
    }
}

impl<N: CrossProduct<D>, D: Copy + Eq> Ord for Ratio<N, D> {
    /// Over one denominator by the numerators alone; else by `num₁ × den₂` against `num₂ × den₁`,
    /// each exact in `N::Product`.
    fn cmp(&self, other: &Ratio<N, D>) -> Ordering {
        if self.den == other.den {
            return self.num.cmp(&other.num);
        }
        self.num.cross(other.den).cmp(&other.num.cross(self.den))
    }
}

impl<N: CrossProduct<D>, D: Copy + Eq> PartialOrd for Ratio<N, D> {
    fn partial_cmp(&self, other: &Ratio<N, D>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Equal in value, as `1/2` and `2/4` are.
impl<N: CrossProduct<D>, D: Copy + Eq> PartialEq for Ratio<N, D> {
    fn eq(&self, other: &Ratio<N, D>) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<N: CrossProduct<D>, D: Copy + Eq> Eq for Ratio<N, D> {}

#[cfg(test)]
mod tests;
