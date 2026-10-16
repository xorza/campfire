use secp256k1::{Secp256k1, VerifyOnly, XOnlyPublicKey, schnorr};

use crate::signature::Signature;
use crate::signed::Signed;

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
        record.write_message(&mut self.message);
        let signature = schnorr::Signature::from_byte_array(signature.to_bytes());
        self.secp
            .verify_schnorr(&signature, &self.message, key)
            .is_ok()
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}
