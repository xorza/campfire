use secp256k1::{Secp256k1, VerifyOnly, XOnlyPublicKey, schnorr};

use crate::signature::Signature;

/// The context that checks signatures, and the scratch a message of no fixed length is written
/// into, cleared and refilled, so no check allocates.
#[derive(Debug)]
pub struct Verifier {
    secp: Secp256k1<VerifyOnly>,
    message: Vec<u8>,
}

impl Verifier {
    pub fn new() -> Verifier {
        Verifier {
            secp: Secp256k1::verification_only(),
            message: Vec::new(),
        }
    }

    /// Whether `signature` is `key`'s over `message`.
    pub(crate) fn verifies(
        &self,
        signature: Signature,
        key: &XOnlyPublicKey,
        message: &[u8],
    ) -> bool {
        let signature = schnorr::Signature::from_byte_array(signature.to_bytes());
        self.secp.verify_schnorr(&signature, message, key).is_ok()
    }

    /// Whether `signature` is `key`'s over the message `write` writes into the scratch, which
    /// it holds alone.
    pub(crate) fn verifies_written(
        &mut self,
        signature: Signature,
        key: &XOnlyPublicKey,
        write: impl FnOnce(&mut Vec<u8>),
    ) -> bool {
        self.message.clear();
        write(&mut self.message);
        let signature = schnorr::Signature::from_byte_array(signature.to_bytes());
        self.secp
            .verify_schnorr(&signature, &self.message, key)
            .is_ok()
    }
}

impl Default for Verifier {
    fn default() -> Verifier {
        Verifier::new()
    }
}
