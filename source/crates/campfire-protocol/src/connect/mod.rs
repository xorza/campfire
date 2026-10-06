use campfire_common::Bytes32;
use derive_more::Display;
use secp256k1::{Keypair, Secp256k1, Signing, Verification};
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
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ConnectChallenge(Bytes32);

impl ConnectChallenge {
    pub const fn new(bytes: [u8; 32]) -> ConnectChallenge {
        ConnectChallenge(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
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
        Signature::sign(secp, session_key, &self.answer_message(certificate), aux)
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
        delegation
            .check(&terms.server_key, &terms.session_id())
            .map_err(ConnectError::Scope)?;
        let granted = delegation.terms();
        if now >= granted.expiration {
            return Err(ConnectError::Expired);
        }
        if !answer.verifies(
            secp,
            &granted.session_key,
            &self.answer_message(certificate),
        ) {
            return Err(ConnectError::BadAnswer);
        }
        Ok(())
    }

    /// `domain ‖ challenge ‖ certificate hash`.
    const fn answer_message(&self, certificate: &CertificateHash) -> [u8; ANSWER_MESSAGE_LEN] {
        let mut message = [0; ANSWER_MESSAGE_LEN];
        let (domain, rest) = message.split_at_mut(ANSWER_DOMAIN.len());
        let (challenge, hash) = rest.split_at_mut(32);
        domain.copy_from_slice(ANSWER_DOMAIN);
        challenge.copy_from_slice(self.0.as_bytes());
        hash.copy_from_slice(certificate.as_bytes());
        message
    }
}

#[cfg(test)]
mod tests;
