/// SplitMix64 (Steele, Lea & Flood, OOPSLA 2014): a seedable generator whose state is one `u64`,
/// so a seeded run, as a simulated link's losses and jitter, replays exactly.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform `f32` in `[0, 1)` from the top 24 bits, which an `f32` mantissa holds exactly.
    pub fn next_f32(&mut self) -> f32 {
        const SCALE: f32 = 1.0 / 16_777_216.0;
        (self.next_u64() >> 40) as f32 * SCALE
    }

    /// A uniform integer in `[0, bound)` by Lemire's multiply-shift, whose bias is below
    /// `bound / 2^64`.
    pub const fn below(&mut self, bound: u64) -> u64 {
        ((self.next_u64() as u128 * bound as u128) >> 64) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_sequence() {
        // Vigna's reference `splitmix64.c` from state 0.
        let mut rng = SplitMix64::new(0);
        let outputs = [(); 4].map(|()| rng.next_u64());
        assert_eq!(
            outputs,
            [
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F,
                0xF88B_B8A8_724C_81EC,
            ]
        );
    }

    #[test]
    fn derived_draws_map_the_raw_output() {
        // 0xE220A8397B1DCDAF >> 40 = 0xE220A8 = 14_819_496, and 14_819_496 / 2^24 is exact.
        assert_eq!(SplitMix64::new(0).next_f32(), 14_819_496.0 / 16_777_216.0);
        // (0xE220A8397B1DCDAF * 10) >> 64 = 8, since 0xE220A8397B1DCDAF / 2^64 = 0.883...
        assert_eq!(SplitMix64::new(0).below(10), 8);
        assert_eq!(SplitMix64::new(0).below(1), 0);
    }
}
