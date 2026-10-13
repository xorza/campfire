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
        self.to_bits() as f32 / (1_u64 << Num::FRAC_BITS) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_draws_as_its_value() {
        // 2.5 and −0.75 are sums of powers of two, exact in an f32.
        assert_eq!(Num::from_bits(5 << (Num::FRAC_BITS - 1)).float(), 2.5);
        assert_eq!(Num::from_bits(-3 << (Num::FRAC_BITS - 2)).float(), -0.75);
        assert_eq!(Num::ZERO.float(), 0.0);
    }
}
