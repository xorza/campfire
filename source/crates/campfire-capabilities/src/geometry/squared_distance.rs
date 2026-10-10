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
    /// By `num₁ × den₂` against `num₂ × den₁`, exact in 384 bits, which a `U256` times a `u128`
    /// never passes: a point's distance to a path, up to 2¹⁸⁴ over 2⁹², against one to a box's
    /// edge passes 256.
    fn cmp(&self, other: &SquaredDistance) -> Ordering {
        if self.den == other.den {
            return self.num.cmp(&other.num);
        }
        self.num.cmp_products(other.den, other.num, self.den)
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
        // A path's distance over 2⁹² against an edge's over 2⁷³, whose cross products pass 256
        // bits: (2¹⁸⁴ + 2⁹¹) / 2⁹² and (2¹⁶⁵ + 2⁷²) / 2⁷³ are both 2⁹² + ½; one more in the first
        // numerator is past the second, both ways.
        let over = |high: u32, half: u32, extra: u128, den: u32| {
            let num = U256::product(1 << high, 1 << high)
                .checked_add(U256::product(1 << half, 1))
                .and_then(|num| num.checked_add(U256::product(extra, 1)));
            SquaredDistance::new(num.unwrap(), 1 << den)
        };
        let path = over(92, 91, 0, 92);
        let edge = SquaredDistance::new(
            U256::product(1 << 82, 1 << 83)
                .checked_add(U256::product(1 << 72, 1))
                .unwrap(),
            1 << 73,
        );
        assert_eq!(path, edge);
        assert_eq!(edge, path);
        let past = over(92, 91, 1, 92);
        assert_eq!(past.cmp(&edge), Ordering::Greater);
        assert_eq!(edge.cmp(&past), Ordering::Less);
        // A whole bit apart, past 256 bits too: 2¹⁸⁴ / 2⁹² against (2¹⁶⁵ + 2⁷³) / 2⁷³, 2⁹² + 1.
        let next = SquaredDistance::new(
            U256::product(1 << 82, 1 << 83)
                .checked_add(U256::product(1 << 73, 1))
                .unwrap(),
            1 << 73,
        );
        assert_eq!(wide.cmp(&next), Ordering::Less);
        assert_eq!(next.cmp(&wide), Ordering::Greater);
    }
}
