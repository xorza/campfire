use secp256k1::{Keypair, Secp256k1, SignOnly, XOnlyPublicKey};
use zeroize::Zeroizing;

use crate::entropy::Entropy;
use crate::signature::Signature;
use crate::signed::{self, Signed};

/// A key that signs, the context its signatures take, and what fills their randomness: each
/// signature takes fresh auxiliary randomness, as BIP-340 recommends against side channels. It
/// is its key's one owner: not `Clone`, shared by `Arc`, and it erases the key as it drops or
/// renews it.
#[derive(Debug)]
pub struct Signer {
    key: Keypair,
    secp: Secp256k1<SignOnly>,
    entropy: Entropy,
}

impl Signer {
    pub fn new(key: Keypair, entropy: Entropy) -> Self {
        Self {
            key,
            secp: Secp256k1::signing_only(),
            entropy,
        }
    }

    /// Takes a new key, from fresh randomness, as a player renews their delegation.
    pub fn renew(&mut self) {
        self.key.non_secure_erase();
        self.key = self.entropy.keypair();
    }

    /// The key's secret, which derives the keys of other protocols, as the server's TLS key.
    pub fn secret_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.key.secret_bytes())
    }

    pub fn public_key(&self) -> XOnlyPublicKey {
        self.key.x_only_public_key().0
    }

    pub const fn entropy(&self) -> Entropy {
        self.entropy
    }

    /// The key's signature over `record`.
    pub fn sign(&self, record: &impl Signed) -> Signature {
        self.sign_with(record, &mut Vec::new())
    }

    /// The key's signature over `record`, its message written into `scratch`, as a signer that
    /// signs often keeps one.
    pub fn sign_with(&self, record: &impl Signed, scratch: &mut Vec<u8>) -> Signature {
        signed::write_message(record, scratch);
        self.sign_message(scratch)
    }

    /// The key's signature over a Nostr event's `id`, as NIP-01 signs an event: the id hashes
    /// its own kind and content.
    pub(crate) fn sign_event_id(&self, id: &[u8; 32]) -> Signature {
        self.sign_message(id)
    }

    /// The key's BIP-340 signature over `message`.
    fn sign_message(&self, message: &[u8]) -> Signature {
        let aux = self.entropy.random();
        let signature = self
            .secp
            .sign_schnorr_with_aux_rand(message, &self.key, &aux);
        Signature::from_bytes(signature.to_byte_array())
    }
}

impl Drop for Signer {
    fn drop(&mut self) {
        self.key.non_secure_erase();
    }
}
