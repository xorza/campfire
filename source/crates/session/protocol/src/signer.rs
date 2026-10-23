use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use campfire_common::{DomainHasher, HashDomain, HashOutput};
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
    aux: AuxDraws,
}

/// Where each signature's auxiliary randomness comes from: BLAKE3 of a secret seed the entropy
/// fills once and a count of the draws, a pseudorandom function in counter mode, as a stream
/// cipher keyed once by the OS draws, so a signature takes no system call; a fixed entropy's
/// byte for every draw, so a test's signatures repeat.
enum AuxDraws {
    Seeded {
        seed: Zeroizing<[u8; 32]>,
        drawn: AtomicU64,
    },
    #[cfg(feature = "internals")]
    Fixed(u8),
}

/// The auxiliary randomness of one signature, erased as it drops.
struct AuxRandom(Zeroizing<[u8; 32]>);

/// The domain of the draws of auxiliary randomness.
const AUX_DOMAIN: HashDomain<AuxRandom> = HashDomain::new("campfire/signature-aux/v1");

impl Signer {
    pub fn new(key: Keypair, entropy: Entropy) -> Self {
        Self {
            key,
            secp: Secp256k1::signing_only(),
            entropy,
            aux: AuxDraws::of(entropy),
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
        let aux = self.aux.draw();
        let signature = self
            .secp
            .sign_schnorr_with_aux_rand(message, &self.key, &aux.0);
        Signature::from_bytes(signature.to_byte_array())
    }
}

impl AuxDraws {
    /// The draws of `entropy`: from a seed it fills now, or its fixed byte.
    fn of(entropy: Entropy) -> Self {
        match entropy {
            Entropy::System => {
                let mut seed = Zeroizing::new([0; 32]);
                entropy.fill(&mut seed);
                Self::Seeded {
                    seed,
                    drawn: AtomicU64::new(0),
                }
            }
            #[cfg(feature = "internals")]
            Entropy::Fixed(byte) => Self::Fixed(byte),
        }
    }

    /// The next draw. Two signatures that share a draw stay sound, as BIP-340's nonce hashes
    /// the key and the message too, so the count's order across threads needs no more than an
    /// atomic step.
    fn draw(&self) -> AuxRandom {
        match self {
            Self::Seeded { seed, drawn } => {
                let count = drawn.fetch_add(1, Ordering::Relaxed);
                let mut hasher = DomainHasher::new(AUX_DOMAIN);
                hasher.update(&**seed).update(&count.to_le_bytes());
                hasher.finish()
            }
            #[cfg(feature = "internals")]
            Self::Fixed(byte) => AuxRandom(Zeroizing::new([*byte; 32])),
        }
    }
}

/// Its seed is a secret, which it shows none of.
impl fmt::Debug for AuxDraws {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuxDraws")
    }
}

/// It is a secret, which it shows none of.
impl fmt::Debug for AuxRandom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuxRandom")
    }
}

impl HashOutput for AuxRandom {
    fn of_hash(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
}

impl Drop for Signer {
    fn drop(&mut self) {
        self.key.non_secure_erase();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_draw_is_blake3_of_the_seed_and_its_count_and_a_fixed_entropy_repeats() {
        // BLAKE3 keyed by the domain's tag, of the seed, then the count as 8 little-endian bytes.
        let seeded = AuxDraws::Seeded {
            seed: Zeroizing::new([5; 32]),
            drawn: AtomicU64::new(0),
        };
        let by_hand = |count: u64| {
            let mut hasher = blake3::Hasher::new_derive_key("campfire/signature-aux/v1");
            hasher.update(&[5; 32]);
            hasher.update(&count.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        assert_eq!(*seeded.draw().0, by_hand(0));
        assert_eq!(*seeded.draw().0, by_hand(1));
        assert_ne!(by_hand(0), by_hand(1));
        let fixed = AuxDraws::of(Entropy::Fixed(7));
        assert_eq!(*fixed.draw().0, [7; 32]);
        assert_eq!(*fixed.draw().0, [7; 32]);
        // The system's entropy seeds its draws: two signers' seeds differ, and one signer's
        // signatures over one message differ by their randomness.
        let key = Entropy::Fixed(3).keypair();
        let system = Signer::new(key, Entropy::System);
        assert_ne!(system.sign_message(b"m"), system.sign_message(b"m"));
        let fixed = Signer::new(key, Entropy::Fixed(3));
        assert_eq!(fixed.sign_message(b"m"), fixed.sign_message(b"m"));
    }
}
