use campfire_common::{ChannelBinding, Meaning, Value32};

use crate::connect::error::ConnectError;
use crate::delegation::Delegation;
use crate::session_terms::SessionTerms;
use crate::signature::Signature;
use crate::signed::{SignatureDomain, Signed};
use crate::verifier::Verifier;

pub(crate) mod error;

/// The random bytes a server sends a connecting client. The client answers with its delegation
/// and its session key's signature over the challenge and its TLS session's channel binding,
/// which binds the answer to this one connection: a party that relays it holds two sessions,
/// whose bindings differ (15-quic-transport.md, The answer binds to the TLS session).
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

/// `domain ‖ challenge ‖ binding`, as the session key signs an answer.
impl Signed for ConnectAnswer<'_> {
    const DOMAIN: SignatureDomain = SignatureDomain::new("campfire/connect/v2");

    fn write_body(&self, message: &mut Vec<u8>) {
        message.extend_from_slice(self.challenge.as_bytes());
        message.extend_from_slice(self.binding.as_bytes());
    }
}

impl ConnectAnswer<'_> {
    /// Checks a joining player's `answer` with `delegation`, at `now` in Unix seconds: the
    /// delegation names the server and the session of `terms` and has not expired, and its
    /// session key signed the challenge over the binding, the server's own end of the TLS
    /// session.
    pub fn check(
        &self,
        verifier: &mut Verifier,
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
        if !verifier.verifies(answer, &granted.session_key, self) {
            return Err(ConnectError::BadAnswer);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
