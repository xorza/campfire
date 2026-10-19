use campfire_math::SplitMix64;

/// A seeded stream of mutations of valid inputs, for a parser's robustness test: each flips a few
/// bits, cuts the input short, extends it, or overwrites a run of it, by a seeded draw, so a
/// failing input replays from its seed (13-net-stack/testing.md, A parser's robustness is a seeded
/// mutation test).
#[derive(Debug, Clone)]
pub struct Mutations {
    random: SplitMix64,
}

impl Mutations {
    pub const fn new(seed: u64) -> Self {
        Self {
            random: SplitMix64::new(seed),
        }
    }

    /// Writes a mutation of `input` into `out`, which it clears first.
    pub fn mutate(&mut self, input: &[u8], out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(input);
        match self.random.below(4) {
            0 => {
                for _ in 0..=self.random.below(4) {
                    if let Some(index) = self.index(out.len()) {
                        out[index] ^= 1 << self.random.below(8);
                    }
                }
            }
            1 => {
                let len = self.below(out.len() + 1);
                out.truncate(len);
            }
            2 => {
                for _ in 0..=self.random.below(16) {
                    out.push(self.byte());
                }
            }
            _ => {
                if let Some(start) = self.index(out.len()) {
                    let run = 1 + self.below((out.len() - start).min(8));
                    for byte in &mut out[start..start + run] {
                        *byte = self.byte();
                    }
                }
            }
        }
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.random.below(u64::try_from(bound).unwrap())).unwrap()
    }

    /// An index into a slice of `len` bytes, none of an empty one.
    fn index(&mut self, len: usize) -> Option<usize> {
        (len > 0).then(|| self.below(len))
    }

    const fn byte(&mut self) -> u8 {
        self.random.next_u64().to_le_bytes()[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One seed gives one stream, another seed another; across 256 mutations of 16 bytes, each
    /// kind shows: shorter, longer, and of the same length but changed.
    #[test]
    fn a_seed_replays_its_mutations_and_each_kind_shows() {
        let input = [0x5a_u8; 16];
        let stream = |seed| {
            let mut mutations = Mutations::new(seed);
            let mut out = Vec::new();
            (0..256)
                .map(|_| {
                    mutations.mutate(&input, &mut out);
                    out.clone()
                })
                .collect::<Vec<_>>()
        };
        let first = stream(1);
        assert_eq!(first, stream(1));
        assert_ne!(first, stream(2));
        assert!(first.iter().any(|out| out.len() < input.len()));
        assert!(first.iter().any(|out| out.len() > input.len()));
        assert!(
            first
                .iter()
                .any(|out| out.len() == input.len() && out[..] != input)
        );
        let mut out = vec![1, 2, 3];
        Mutations::new(3).mutate(&[], &mut out);
        assert!(out.len() <= 17, "an empty input mutates without a panic");
    }
}
