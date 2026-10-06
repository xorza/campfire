use secp256k1::{Keypair, Secp256k1, SecretKey};

/// The tests' fixed keys: the key of one byte repeated, and the key the tests' servers sign with.
#[derive(Debug)]
pub struct TestKey;

impl TestKey {
    /// The byte the key of the tests' servers repeats.
    const SERVER: u8 = 41;

    /// The key whose secret is 32 bytes of `byte`: every byte but 0 and 255 gives one, as 32
    /// bytes of 255 pass the curve's order.
    pub fn of(byte: u8) -> Keypair {
        let secret = SecretKey::from_byte_array(&[byte; 32]).expect("a secret key below the order");
        Keypair::from_secret_key(&Secp256k1::new(), &secret)
    }

    /// The key the tests' servers sign with.
    pub fn server() -> Keypair {
        TestKey::of(TestKey::SERVER)
    }
}
