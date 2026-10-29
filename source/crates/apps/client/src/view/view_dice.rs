use campfire_math::SplitMix64;

/// What the client draws for what it shows from a seed, as a look's pick, an effect's spread or
/// a sound's loss: `SplitMix64`'s words, as `campfire_math` draws them, none of them the sim's.
#[derive(Debug, Clone)]
pub(crate) struct ViewDice(SplitMix64);

impl ViewDice {
    pub(crate) const fn new(seed: u64) -> Self {
        Self(SplitMix64::new(seed))
    }

    /// A whole number below `count`, which is above 0, uniformly, as `SplitMix64::below` draws.
    pub(crate) fn below(&mut self, count: usize) -> usize {
        let count = u64::try_from(count).expect("a usize fits u64");
        usize::try_from(self.0.below(count)).expect("below a usize")
    }

    /// A share from 0 up to 1, 1 not among them, of 24 bits of a draw, which an `f32` holds whole.
    #[expect(clippy::cast_precision_loss, reason = "24 bits fit an f32 whole")]
    pub(crate) fn share(&mut self) -> f32 {
        (self.0.next_u64() >> 40) as f32 / (1_u32 << 24) as f32
    }

    /// A number from `least` to `most`, uniformly, as `share` draws.
    pub(crate) fn between(&mut self, least: f32, most: f32) -> f32 {
        least + (most - least) * self.share()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draw_is_splitmix64_s_of_its_seed() {
        // SplitMix64 of seed 0 first gives 0xE220A8397B1DCDAF, whose top 24 bits are 0xE220A8,
        // a share of 14819496 / 2²⁴.
        let mut dice = ViewDice::new(0);
        assert_eq!(dice.share(), 14_819_496.0 / 16_777_216.0);
        // Seed 7's first three words, times 3, fall in thirds 1, 0 and 2 of 2⁶⁴, none rejected.
        let mut dice = ViewDice::new(7);
        assert_eq!([(); 3].map(|()| dice.below(3)), [1, 0, 2]);
        let mut dice = ViewDice::new(0);
        assert_eq!(
            dice.between(2.0, 4.0),
            2.0 + 2.0 * (14_819_496.0 / 16_777_216.0)
        );
    }
}
