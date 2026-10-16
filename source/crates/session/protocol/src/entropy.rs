use secp256k1::{Keypair, Secp256k1, SecretKey};
use zeroize::Zeroizing;

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
            Self::System => getrandom::fill(bytes).expect("the OS gives random bytes"),
            #[cfg(feature = "internals")]
            Self::Fixed(byte) => bytes.fill(byte),
        }
    }

    /// 32 bytes it fills.
    pub fn random(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        self.fill(&mut bytes);
        bytes
    }

    /// A new key of the bytes it fills, as a player's session key, or a key a key file holds,
    /// is made.
    pub fn keypair(self) -> Keypair {
        keypair_of(|bytes| self.fill(bytes))
    }
}

/// The key of the first 32 bytes from `fill` that are a secret key: zero and the values from
/// the curve's order up are not, a 2⁻¹²⁸ share of draws. Each draw is zeroed as it drops.
fn keypair_of(mut fill: impl FnMut(&mut [u8; 32])) -> Keypair {
    loop {
        let mut secret = Zeroizing::new([0; 32]);
        fill(&mut secret);
        if let Ok(secret) = SecretKey::from_byte_array(&secret) {
            return Keypair::from_secret_key(&Secp256k1::new(), &secret);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_skips_bytes_that_are_no_secret_key() {
        // Zero is no secret key: the next draw, 32 bytes of 7, is the key.
        let mut draws = [[0; 32], [7; 32]].into_iter();
        let key = keypair_of(|bytes| *bytes = draws.next().unwrap());
        let seven = SecretKey::from_byte_array(&[7; 32]).unwrap();
        assert_eq!(key, Keypair::from_secret_key(&Secp256k1::new(), &seven));
        assert_eq!(draws.next(), None);
        assert_eq!(Entropy::Fixed(7).keypair(), key);
    }

    #[test]
    fn fixed_entropy_fills_its_byte_and_the_systems_differs_between_draws() {
        assert_eq!(Entropy::Fixed(7).random(), [7; 32]);
        assert_ne!(Entropy::System.random(), Entropy::System.random());
    }
}
