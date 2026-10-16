/// Four lanes of a comparison: each all ones where it holds and 0 where it does not, the form
/// AVX2's and NEON's comparisons give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mask64x4([i64; 4]);

impl Mask64x4 {
    /// The mask of `holds`, lane by lane.
    pub(crate) const fn from_holds(holds: [bool; 4]) -> Self {
        Self([
            Self::lane(holds[0]),
            Self::lane(holds[1]),
            Self::lane(holds[2]),
            Self::lane(holds[3]),
        ])
    }

    /// Bit `i` set where lane `i` holds.
    pub const fn to_bits(self) -> u8 {
        let [a, b, c, d] = self.0;
        Self::sign(a) | (Self::sign(b) << 1) | (Self::sign(c) << 2) | (Self::sign(d) << 3)
    }

    const fn lane(holds: bool) -> i64 {
        (holds as i64).wrapping_neg()
    }

    const fn sign(lane: i64) -> u8 {
        (lane.cast_unsigned() >> 63) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bits_follow_each_lane() {
        let a = Mask64x4::from_holds([true, false, true, true]);
        let b = Mask64x4::from_holds([true, true, false, true]);
        assert_eq!(a.0, [-1, 0, -1, -1]);
        assert_eq!(a.to_bits(), 0b1101);
        assert_eq!(b.to_bits(), 0b1011);
        assert_eq!(Mask64x4::from_holds([false; 4]).to_bits(), 0);
        assert_eq!(Mask64x4::from_holds([true; 4]).to_bits(), 0b1111);
    }
}
