use campfire_math::Num;

/// A sim number as the renderer draws it. The sim's core does no float arithmetic, so the
/// conversion lives here, with the drawing that needs it.
pub(crate) trait FloatNum {
    /// The number in an `f32`, which keeps 24 bits of it, all a drawing needs of a place.
    fn float(self) -> f32;
}

impl FloatNum for Num {
    #[expect(
        clippy::cast_precision_loss,
        reason = "drawing needs no more than an f32's 24 bits of a place"
    )]
    fn float(self) -> f32 {
        self.to_bits() as f32 / (1_u64 << Self::FRAC_BITS) as f32
    }
}

/// 2⁶³, past the bits of every `Num`, exact in an `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "2⁶³ is a power of two, exact in an f64"
)]
const BITS_LIMIT: f64 = (1_u64 << 63) as f64;

/// The sim number nearest a drawn value, a tie to the even one: exact for each value a step of
/// 2⁻²⁴ holds, as every `f32` of at least 2⁻¹ that fits; `None` for one not finite or past what
/// a `Num` holds. An `f32` widens to an `f64` exactly, and the power of two scales it exactly, so
/// the one rounding is the last.
pub(crate) fn to_num(value: f32) -> Option<Num> {
    let bits = (f64::from(value) * f64::from(1_u32 << Num::FRAC_BITS)).round_ties_even();
    (-BITS_LIMIT..BITS_LIMIT)
        .contains(&bits)
        .then(|| Num::from_bits(bits as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_value_is_the_nearest_number() {
        let bits = |value: f32| to_num(value).map(Num::to_bits);
        // 1234.5677 in `f32` is 1234.5677490234375, 20,712,609,792 steps of 2⁻²⁴; −0.625 is
        // 10,485,760 steps below 0.
        assert_eq!(bits(1234.5677), Some(20_712_609_792));
        assert_eq!(bits(-0.625), Some(-10_485_760));
        // 2⁻²⁵ is half a step and rounds to the even 0; 1.5 · 2⁻²⁴ to the even 2; 1.25 · 2⁻²⁴
        // to 1; −1.5 · 2⁻²⁴ to −2; the smallest subnormal and −0 to 0.
        let step = f32::from_bits(0x3380_0000);
        assert_eq!(bits(step / 2.0), Some(0));
        assert_eq!(bits(step * 1.5), Some(2));
        assert_eq!(bits(step * 1.25), Some(1));
        assert_eq!(bits(step * -1.5), Some(-2));
        assert_eq!(bits(f32::from_bits(1)), Some(0));
        assert_eq!(bits(-0.0), Some(0));
        // −2³⁹ is the least `Num`; 2³⁹ is one past the largest, as are the larger and the values
        // not finite.
        let edge = f32::from_bits(0x5300_0000);
        assert_eq!(bits(-edge), Some(i64::MIN));
        assert_eq!(bits(edge), None);
        assert_eq!(bits(f32::MAX), None);
        assert_eq!(bits(f32::NAN), None);
        assert_eq!(bits(f32::NEG_INFINITY), None);
    }

    #[test]
    fn a_number_draws_as_its_value() {
        // 2.5 and −0.75 are sums of powers of two, exact in an f32.
        assert_eq!(Num::from_bits(5 << (Num::FRAC_BITS - 1)).float(), 2.5);
        assert_eq!(Num::from_bits(-3 << (Num::FRAC_BITS - 2)).float(), -0.75);
        assert_eq!(Num::ZERO.float(), 0.0);
    }
}
