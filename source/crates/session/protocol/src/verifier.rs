use secp256k1::{Secp256k1, VerifyOnly, XOnlyPublicKey, schnorr};

use crate::signature::Signature;
use crate::signed::{self, Signed};

/// The context that checks signatures, and the scratch a message of no fixed length is written
/// into, cleared and refilled, so no check allocates.
#[derive(Debug)]
pub struct Verifier {
    secp: Secp256k1<VerifyOnly>,
    message: Vec<u8>,
}

impl Verifier {
    pub fn new() -> Self {
        Self {
            secp: Secp256k1::verification_only(),
            message: Vec::new(),
        }
    }

    /// Whether `signature` is `key`'s over `record`, whose message it writes into its scratch.
    pub fn verifies(
        &mut self,
        signature: &Signature,
        key: &XOnlyPublicKey,
        record: &impl Signed,
    ) -> bool {
        let mut message = std::mem::take(&mut self.message);
        signed::write_message(record, &mut message);
        let verifies = self.verifies_message(signature, key, &message);
        self.message = message;
        verifies
    }

    /// Whether `signature` is `key`'s over `message`, a record's as `Signed` writes it.
    pub(crate) fn verifies_message(
        &self,
        signature: &Signature,
        key: &XOnlyPublicKey,
        message: &[u8],
    ) -> bool {
        let signature = schnorr::Signature::from_byte_array(signature.to_bytes());
        self.secp.verify_schnorr(&signature, message, key).is_ok()
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}
