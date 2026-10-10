use crate::floor_root::FloorRoot;

/// The exact integer square root, rounded up.
pub trait CeilRoot {
    /// The least integer whose square is at least `self`.
    #[must_use]
    fn ceil_root(self) -> Self;
}

impl CeilRoot for u128 {
    fn ceil_root(self) -> u128 {
        let root = self.floor_root();
        // The floor root is below 2⁶⁴, so its square fits. The step sits in an `if`: a select in
        // its place made `collision/crowded` 4 % slower.
        if root * root < self { root + 1 } else { root }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_rounds_up_to_the_least_whole_root() {
        // 0, 1 and 4 are squares; 2, 3 and 5 round up to 2, 2 and 3.
        let small = [0_u128, 1, 2, 3, 4, 5].map(CeilRoot::ceil_root);
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
}
