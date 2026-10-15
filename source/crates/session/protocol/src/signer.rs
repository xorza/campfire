use secp256k1::{Keypair, Secp256k1, SignOnly, XOnlyPublicKey};

use crate::entropy::Entropy;
use crate::random_key::RandomKey;
use crate::signature::Signature;

/// A key that signs, the context its signatures take, and what fills their randomness: each
/// signature takes fresh auxiliary randomness, as BIP-340 recommends against side channels.
#[derive(Debug, Clone)]
pub struct Signer {
    key: Keypair,
    secp: Secp256k1<SignOnly>,
    entropy: Entropy,
}

impl Signer {
    pub fn new(key: Keypair, entropy: Entropy) -> Signer {
        Signer {
            key,
            secp: Secp256k1::signing_only(),
            entropy,
        }
    }

    /// Takes a new key, from fresh randomness, as a player renews their delegation.
    pub fn renew(&mut self) {
        let entropy = self.entropy;
        self.key = RandomKey::generate(|bytes| entropy.fill(bytes));
    }

    pub fn public_key(&self) -> XOnlyPublicKey {
        self.key.x_only_public_key().0
    }

    pub const fn entropy(&self) -> Entropy {
        self.entropy
    }

    /// The key's BIP-340 signature over `message`.
    pub(crate) fn sign(&self, message: &[u8]) -> Signature {
        let aux = self.entropy.random();
        let signature = self
            .secp
            .sign_schnorr_with_aux_rand(message, &self.key, &aux);
        Signature::from_bytes(signature.to_byte_array())
    }
}
