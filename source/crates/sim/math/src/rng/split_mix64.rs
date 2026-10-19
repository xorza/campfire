use crate::num::Num;
use crate::rng::{chance_from_word, fraction_of_word, lemire_step};

/// `SplitMix64`, Steele, Lea and Flood's generator: fast, well-mixed words that every platform
/// draws alike from one seed, for what a seed draws outside a match: a test's inputs, the network
/// emulator's losses and delays, and a seeded run's random bytes. The sim draws from `Rng`, never
/// from this. Its draws mean what `Rng`'s mean, from its words.
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
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }

    /// A uniform integer in `[0, bound)`, by Lemire's multiply-and-reject, as `Rng::below` draws;
    /// `bound` is not zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        debug_assert!(bound != 0, "SplitMix64::below(0)");
        loop {
            if let Some(value) = lemire_step::<64>(self.next_u64(), bound) {
                return value;
            }
        }
    }

    /// True with probability exactly `probability`, clamped to `[0, 1]`, as `Rng::chance` draws.
    pub const fn chance(&mut self, probability: Num) -> bool {
        chance_from_word(self.next_u64(), probability)
    }

    /// A uniform number in `[0, 1)` of a `Num`'s 24 fractional bits, as `Rng::fraction` draws.
    pub const fn fraction(&mut self) -> Num {
        fraction_of_word(self.next_u64())
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

    #[test]
    fn a_draw_reads_its_word_as_rng_reads_one() {
        // Seed 0's first word is 0xE220A8397B1DCDAF, 0.883… of 2^64: its product with 10 is
        // 8.83… · 2^64, so `below(10)` is 8, and its low 64 bits are far from the rejection band.
        assert_eq!(SplitMix64::new(0).below(10), 8);
        assert_eq!(SplitMix64::new(0).below(1), 0);
        // Its top 24 bits are 0xE220A8, 14_819_496 of 2^24, the fraction's bits.
        assert_eq!(SplitMix64::new(0).fraction(), Num::from_bits(0xE2_20A8));
        assert!(SplitMix64::new(0).chance(Num::from_bits(0xE2_20A9)));
        assert!(!SplitMix64::new(0).chance(Num::from_bits(0xE2_20A8)));
        assert!(!SplitMix64::new(0).chance(Num::ZERO) && SplitMix64::new(0).chance(Num::ONE));
    }
}
