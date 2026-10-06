use secp256k1::{Keypair, Secp256k1, SecretKey};

/// A new key from random bytes, as a player's session key, or a key a key file holds, is made.
#[derive(Debug)]
pub struct RandomKey;

impl RandomKey {
    /// The key of the first 32 bytes from `fill` that are a secret key: zero and the values from
    /// the curve's order up are not, a 2⁻¹²⁸ share of draws.
    pub fn generate(mut fill: impl FnMut(&mut [u8; 32])) -> Keypair {
        loop {
            let mut secret = [0; 32];
            fill(&mut secret);
            if let Ok(secret) = SecretKey::from_byte_array(&secret) {
                return Keypair::from_secret_key(&Secp256k1::new(), &secret);
            }
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
        let key = RandomKey::generate(|bytes| *bytes = draws.next().unwrap());
        let seven = SecretKey::from_byte_array(&[7; 32]).unwrap();
        assert_eq!(key, Keypair::from_secret_key(&Secp256k1::new(), &seven));
        assert_eq!(draws.next(), None);
    }
}
