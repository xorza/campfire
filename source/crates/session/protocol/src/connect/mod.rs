use campfire_common::{ChannelBinding, Meaning, Value32};

use crate::connect::error::ConnectError;
use crate::delegation::Delegation;
use crate::session_terms::SessionTerms;
use crate::signature::Signature;
use crate::signer::Signer;
use crate::verifier::Verifier;

pub(crate) mod error;

/// Starts every signed connect answer, so no other signature of the session key counts as one.
const ANSWER_DOMAIN: &[u8] = b"campfire/connect/v2";
const ANSWER_MESSAGE_LEN: usize = ANSWER_DOMAIN.len() + 32 + 32;

/// The random bytes a server sends a connecting client. The client answers with its delegation
/// and its session key's signature over the challenge and its TLS session's channel binding,
/// which binds the answer to this one connection: a party that relays it holds two sessions,
/// whose bindings differ (design 15, D11).
pub type ConnectChallenge = Value32<ConnectChallengeMeaning>;

/// The meaning of a `ConnectChallenge`.
#[derive(Debug)]
pub enum ConnectChallengeMeaning {}

impl Meaning for ConnectChallengeMeaning {
    const NAME: &'static str = "ConnectChallenge";
}

/// What a connecting client's session key signs: the server's `challenge` over `binding`, its TLS
/// session's channel binding.
#[derive(Debug, Clone, Copy)]
pub struct ConnectAnswer<'a> {
    pub challenge: &'a ConnectChallenge,
    pub binding: &'a ChannelBinding,
}

impl ConnectAnswer<'_> {
    /// The answer as the session key, `session`'s, signs it.
    pub fn sign(&self, session: &Signer) -> Signature {
        session.sign(&self.message())
    }

    /// Checks a joining player's `answer` with `delegation`, at `now` in Unix seconds: the
    /// delegation names the server and the session of `terms` and has not expired, and its
    /// session key signed the challenge over the binding, the server's own end of the TLS
    /// session.
    pub fn check(
        &self,
        verifier: &Verifier,
        terms: &SessionTerms,
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
        if !verifier.verifies(*answer, &granted.session_key, &self.message()) {
            return Err(ConnectError::BadAnswer);
        }
        Ok(())
    }

    /// `domain ‖ challenge ‖ binding`.
    const fn message(&self) -> [u8; ANSWER_MESSAGE_LEN] {
        let mut message = [0; ANSWER_MESSAGE_LEN];
        let (domain, rest) = message.split_at_mut(ANSWER_DOMAIN.len());
        let (challenge, bound) = rest.split_at_mut(32);
        domain.copy_from_slice(ANSWER_DOMAIN);
        challenge.copy_from_slice(self.challenge.as_bytes());
        bound.copy_from_slice(self.binding.as_bytes());
        message
    }
}

#[cfg(test)]
mod tests;
