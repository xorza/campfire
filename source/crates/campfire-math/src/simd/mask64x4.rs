/// Four lanes of a comparison: each all ones where it holds and 0 where it does not, the form
/// AVX2's and NEON's comparisons give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mask64x4([i64; 4]);

impl Mask64x4 {
    /// The mask of `holds`, lane by lane.
    pub(crate) const fn from_holds(holds: [bool; 4]) -> Mask64x4 {
        Mask64x4([
            Mask64x4::lane(holds[0]),
            Mask64x4::lane(holds[1]),
            Mask64x4::lane(holds[2]),
            Mask64x4::lane(holds[3]),
        ])
    }

    /// Holds where both hold.
    #[must_use]
    pub const fn and(self, rhs: Mask64x4) -> Mask64x4 {
        let [a, b] = [self.0, rhs.0];
        Mask64x4([a[0] & b[0], a[1] & b[1], a[2] & b[2], a[3] & b[3]])
    }

    /// Bit `i` set where lane `i` holds.
    pub const fn to_bits(self) -> u8 {
        let [a, b, c, d] = self.0;
        Mask64x4::sign(a) | Mask64x4::sign(b) << 1 | Mask64x4::sign(c) << 2 | Mask64x4::sign(d) << 3
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
    fn the_bits_and_their_and_follow_each_lane() {
        let a = Mask64x4::from_holds([true, false, true, true]);
        let b = Mask64x4::from_holds([true, true, false, true]);
        assert_eq!(a.0, [-1, 0, -1, -1]);
        assert_eq!(a.to_bits(), 0b1101);
        assert_eq!(b.to_bits(), 0b1011);
        assert_eq!(a.and(b).to_bits(), 0b1001);
        assert_eq!(Mask64x4::from_holds([false; 4]).to_bits(), 0);
        assert_eq!(Mask64x4::from_holds([true; 4]).to_bits(), 0b1111);
    }
}
