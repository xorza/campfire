/// `SplitMix64`, Steele, Lea and Flood's generator: fast, well-mixed words that every platform
/// draws alike from one seed, for the inputs of tests, benches and test harnesses. The sim draws
/// from `Rng`, never from this.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub const fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_the_reference_implementations() {
        // The first words of seed 0 in Vigna's `splitmix64.c`.
        let mut words = SplitMix64::new(0);
        assert_eq!(words.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(words.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(words.next_u64(), 0x06C4_5D18_8009_454F);
    }
}
