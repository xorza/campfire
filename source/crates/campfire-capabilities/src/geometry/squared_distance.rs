use std::cmp::Ordering;

use campfire_math::{Num, U256};

/// A squared distance on the ground plane, `num / den` in squared bits of a `Num`, exact: a
/// point's distance to a box's edge is a cross product's square over the edge's squared length,
/// and to a path a cross product's square over the path's. Compared by cross products.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SquaredDistance {
    num: U256,
    den: u128,
}

impl SquaredDistance {
    pub(crate) const ZERO: SquaredDistance = SquaredDistance {
        num: U256::ZERO,
        den: 1,
    };

    /// `num / den`, `den` positive.
    pub(crate) const fn new(num: U256, den: u128) -> SquaredDistance {
        debug_assert!(den > 0, "a squared distance's denominator is positive");
        SquaredDistance { num, den }
    }

    /// A whole square of bits, as a point's to a corner is.
    pub(crate) const fn whole(square: u128) -> SquaredDistance {
        SquaredDistance {
            num: U256::product(square, 1),
            den: 1,
        }
    }

    /// The square of `reach`, not negative.
    pub(crate) const fn of(reach: Num) -> SquaredDistance {
        debug_assert!(reach.to_bits() >= 0, "a reach is not negative");
        let bits = reach.to_bits().unsigned_abs() as u128;
        SquaredDistance {
            num: U256::product(bits, bits),
            den: 1,
        }
    }
}

impl Ord for SquaredDistance {
    /// By `num₁ × den₂` against `num₂ × den₁`. The distances a box makes keep each within 256
    /// bits: a point's to an edge is at most 2¹⁵⁶ over 2⁶³, a point's to a path at most 2¹⁸⁴
    /// over 2⁹², and two of the second kind compare only on one path, whose denominators match.
    fn cmp(&self, other: &SquaredDistance) -> Ordering {
        if self.den == other.den {
            return self.num.cmp(&other.num);
        }
        let ours = self.num.checked_mul(other.den);
        let theirs = other.num.checked_mul(self.den);
        let (Some(ours), Some(theirs)) = (ours, theirs) else {
            panic!("a box's squared distances compare within 256 bits");
        };
        ours.cmp(&theirs)
    }
}

impl PartialOrd for SquaredDistance {
    fn partial_cmp(&self, other: &SquaredDistance) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Equal in value, whatever the denominators.
impl PartialEq for SquaredDistance {
    fn eq(&self, other: &SquaredDistance) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for SquaredDistance {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squared_distances_compare_by_value() {
        let d = |num: u128, den: u128| SquaredDistance::new(U256::product(num, 1), den);
        // 9/4 = 18/8; 2 < 9/4 < 3; a whole square and a reach's square are over 1.
        assert_eq!(d(9, 4), d(18, 8));
        assert!(SquaredDistance::whole(2) < d(9, 4) && d(9, 4) < SquaredDistance::whole(3));
        assert_eq!(
            SquaredDistance::of(Num::ONE),
            SquaredDistance::whole(1 << 48)
        );
        assert_eq!(SquaredDistance::ZERO, d(0, 5));
        // At the bound: 2¹⁸⁴ over 2⁹² equals 2⁹² over 1, and one more in the numerator is past
        // it; two over the same large denominator compare by numerator alone.
        let wide = SquaredDistance::new(U256::product(1 << 92, 1 << 92), 1 << 92);
        assert_eq!(wide, SquaredDistance::whole(1 << 92));
        let above = U256::product(1 << 92, 1 << 92).checked_add(U256::product(1, 1));
        assert!(SquaredDistance::new(above.unwrap(), 1 << 92) > wide);
    }
}
