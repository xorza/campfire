/// What fills a key, a seed contribution, a challenge or BIP-340's auxiliary randomness.
#[derive(Debug, Clone, Copy)]
pub enum Entropy {
    System,
    /// Every byte this one, so a test's keys and signatures repeat.
    #[cfg(feature = "internals")]
    Fixed(u8),
}

impl Entropy {
    pub fn fill(self, bytes: &mut [u8; 32]) {
        match self {
            Entropy::System => getrandom::fill(bytes).expect("the OS gives random bytes"),
            #[cfg(feature = "internals")]
            Entropy::Fixed(byte) => bytes.fill(byte),
        }
    }

    /// 32 bytes it fills.
    pub fn random(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        self.fill(&mut bytes);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_entropy_fills_its_byte_and_the_systems_differs_between_draws() {
        assert_eq!(Entropy::Fixed(7).random(), [7; 32]);
        assert_ne!(Entropy::System.random(), Entropy::System.random());
    }
}
