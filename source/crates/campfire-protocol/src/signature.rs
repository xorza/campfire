use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey, schnorr};
use serde::{Deserialize, Serialize};

/// A session key's BIP-340 signature, `R.x ‖ s`: over a player's chain head, or over a connect
/// challenge. Kept as its two halves because serde encodes arrays of at most 32 elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    r: [u8; 32],
    s: [u8; 32],
}

impl Signature {
    /// `key`'s BIP-340 signature over `message`, with the auxiliary randomness `aux`.
    pub(crate) fn sign<C: Signing>(
        secp: &Secp256k1<C>,
        key: &Keypair,
        message: &[u8],
        aux: &[u8; 32],
    ) -> Signature {
        let signature = secp.sign_schnorr_with_aux_rand(message, key, aux);
        Signature::from_bytes(signature.to_byte_array())
    }

    /// Whether this is `key`'s signature over `message`.
    pub(crate) fn verifies<C: Verification>(
        self,
        secp: &Secp256k1<C>,
        key: &XOnlyPublicKey,
        message: &[u8],
    ) -> bool {
        let signature = schnorr::Signature::from_byte_array(self.to_bytes());
        secp.verify_schnorr(&signature, message, key).is_ok()
    }

    pub const fn from_bytes(bytes: [u8; 64]) -> Signature {
        let (r, s) = bytes.split_at(32);
        let mut signature = Signature {
            r: [0; 32],
            s: [0; 32],
        };
        signature.r.copy_from_slice(r);
        signature.s.copy_from_slice(s);
        signature
    }

    pub const fn to_bytes(self) -> [u8; 64] {
        let mut bytes = [0; 64];
        let (r, s) = bytes.split_at_mut(32);
        r.copy_from_slice(&self.r);
        s.copy_from_slice(&self.s);
        bytes
    }
}
