use secp256k1::{Keypair, Secp256k1, Signing, Verification, schnorr};
use serde::{Deserialize, Serialize};

use crate::connect::certificate_hash::CertificateHash;
use crate::connect::error::ConnectError;
use crate::delegation::Delegation;
use crate::session_terms::SessionTerms;
use crate::signature::Signature;

pub(crate) mod certificate_hash;
pub(crate) mod error;

/// Starts every signed connect answer, so no other signature of the session key counts as one.
const ANSWER_DOMAIN: &[u8] = b"campfire/connect/v1";
const ANSWER_MESSAGE_LEN: usize = ANSWER_DOMAIN.len() + 32 + 32;

/// The random bytes a server sends a connecting client. The client answers with its delegation
/// and its session key's signature over the challenge and the certificate hash it verified, which
/// binds the answer to this connection to this server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectChallenge([u8; 32]);

impl ConnectChallenge {
    pub const fn new(bytes: [u8; 32]) -> ConnectChallenge {
        ConnectChallenge(bytes)
    }

    /// `session_key`'s answer to the challenge, over `certificate`, the hash the client verified,
    /// with BIP-340's auxiliary randomness `aux`.
    pub fn answer<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        session_key: &Keypair,
        certificate: &CertificateHash,
        aux: &[u8; 32],
    ) -> Signature {
        let message = self.answer_message(certificate);
        let signature = secp.sign_schnorr_with_aux_rand(&message, session_key, aux);
        Signature::from_bytes(signature.to_byte_array())
    }

    /// Checks a joining player's `answer` with `delegation`, at `now` in Unix seconds: the
    /// delegation names the server and the session of `terms` and has not expired, and its
    /// session key signed the challenge over `certificate`, the server's own hash.
    pub fn check<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        terms: &SessionTerms,
        certificate: &CertificateHash,
        delegation: &Delegation,
        answer: &Signature,
        now: u64,
    ) -> Result<(), ConnectError> {
        let granted = delegation.terms();
        if granted.server_key != terms.server_key {
            return Err(ConnectError::OtherServer);
        }
        if granted.session_id != terms.session_id() {
            return Err(ConnectError::OtherSession);
        }
        if now >= granted.expiration {
            return Err(ConnectError::Expired);
        }
        secp.verify_schnorr(
            &schnorr::Signature::from_byte_array(answer.to_bytes()),
            &self.answer_message(certificate),
            &granted.session_key,
        )
        .ok()
        .ok_or(ConnectError::BadAnswer)
    }

    /// `domain ‖ challenge ‖ certificate hash`.
    fn answer_message(&self, certificate: &CertificateHash) -> [u8; ANSWER_MESSAGE_LEN] {
        let mut message = [0; ANSWER_MESSAGE_LEN];
        let (domain, rest) = message.split_at_mut(ANSWER_DOMAIN.len());
        let (challenge, hash) = rest.split_at_mut(32);
        domain.copy_from_slice(ANSWER_DOMAIN);
        challenge.copy_from_slice(&self.0);
        hash.copy_from_slice(certificate.as_bytes());
        message
    }
}

#[cfg(test)]
mod tests;
