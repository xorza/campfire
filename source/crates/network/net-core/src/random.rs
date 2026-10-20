use campfire_math::SplitMix64;

/// Where random bytes come from: the OS in a real run, through `getrandom`, the one source of the
/// workspace's OS random bytes, and a seed in an emulated or tested one, so a run of a seed
/// repeats (02-engine-core.md, One source of OS random bytes).
#[derive(Debug)]
pub enum Random {
    Os,
    Seeded(SplitMix64),
}

impl Random {
    pub const fn os() -> Self {
        Self::Os
    }

    pub const fn seeded(seed: u64) -> Self {
        Self::Seeded(SplitMix64::new(seed))
    }

    /// Fills `bytes`; a seed gives each 8 of them, and the rest at the end, from the
    /// little-endian bytes of its next `u64`.
    pub fn fill(&mut self, bytes: &mut [u8]) {
        match self {
            Self::Os => getrandom::fill(bytes).expect("the OS gives random bytes"),
            Self::Seeded(random) => {
                for chunk in bytes.chunks_mut(8) {
                    let word = random.next_u64().to_le_bytes();
                    chunk.copy_from_slice(&word[..chunk.len()]);
                }
            }
        }
    }

    pub fn array<const N: usize>(&mut self) -> [u8; N] {
        let mut bytes = [0; N];
        self.fill(&mut bytes);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seed gives its words' bytes, the same at each run, and another seed others; the OS gives
    /// bytes too.
    #[test]
    fn a_seed_gives_its_words_in_little_endian() {
        // SplitMix64's first two words from 0, 0xE220A8397B1DCDAF and 0x6E789E6AA1B965F4: all 8
        // bytes of the first, low byte first, then the low 4 of the second.
        assert_eq!(
            Random::seeded(0).array::<12>(),
            [
                0xAF, 0xCD, 0x1D, 0x7B, 0x39, 0xA8, 0x20, 0xE2, 0xF4, 0x65, 0xB9, 0xA1
            ]
        );
        let bytes = |seed| Random::seeded(seed).array::<12>();
        assert_eq!(bytes(1), bytes(1));
        assert_ne!(bytes(0), bytes(1));
        let _ = Random::os().array::<12>();
    }
}
