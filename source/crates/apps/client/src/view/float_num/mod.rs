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
mod tests;
