use campfire_common::{StateHash, Tick};
use secp256k1::{Keypair, Secp256k1, Signing, Verification, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

use crate::session_id::SessionId;
use crate::signature::Signature;

/// Starts every signed result, so no other signature of the server key counts as one.
const SIGNATURE_DOMAIN: &[u8] = b"campfire/result/v1";

/// How a session ended, which the server signs as the last record of its log: the tick the
/// session stopped before, how it ended, and the state hash after the tick before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResult {
    pub tick: Tick,
    pub outcome: Outcome,
    pub state_hash: StateHash,
}

/// How a session ended: its match won by the team of index `team` or drawn, as the mode ended
/// it, or aborted before the mode ended it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Won { team: u8 },
    Draw,
    Aborted,
}

impl SessionResult {
    /// The server key's signature over the result, with BIP-340's auxiliary randomness `aux`.
    pub fn sign<C: Signing>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &Keypair,
        session_id: SessionId,
        aux: &[u8; 32],
    ) -> Signature {
        Signature::sign(secp, server_key, &self.message(session_id), aux)
    }

    /// Whether `signature` is `server_key`'s over the result.
    pub(crate) fn signed_by<C: Verification>(
        &self,
        secp: &Secp256k1<C>,
        server_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &Signature,
    ) -> bool {
        signature.verifies(secp, server_key, &self.message(session_id))
    }

    /// `domain ‖ session id ‖ postcard of the result`.
    fn message(&self, session_id: SessionId) -> Vec<u8> {
        let mut message = Vec::with_capacity(SIGNATURE_DOMAIN.len() + 32 + 48);
        message.extend_from_slice(SIGNATURE_DOMAIN);
        message.extend_from_slice(session_id.as_bytes());
        postcard::to_io(self, &mut message).expect("postcard into a Vec cannot fail");
        message
    }
}

#[cfg(test)]
mod tests {
    use secp256k1::SecretKey;

    use super::*;

    #[test]
    fn a_results_signature_holds_only_over_it() {
        let secp = Secp256k1::new();
        let server =
            Keypair::from_secret_key(&secp, &SecretKey::from_byte_array(&[41; 32]).unwrap());
        let key = server.x_only_public_key().0;
        let id = SessionId::new([8; 32]);
        let result = SessionResult {
            tick: Tick::new(7),
            outcome: Outcome::Won { team: 1 },
            state_hash: StateHash::new([3; 32]),
        };
        let signature = result.sign(&secp, &server, id, &[0; 32]);
        assert!(result.signed_by(&secp, &key, id, &signature));
        assert!(!result.signed_by(&secp, &key, SessionId::new([9; 32]), &signature));
        let others = [
            SessionResult {
                tick: Tick::new(8),
                ..result
            },
            SessionResult {
                outcome: Outcome::Won { team: 0 },
                ..result
            },
            SessionResult {
                outcome: Outcome::Aborted,
                ..result
            },
            SessionResult {
                state_hash: StateHash::new([4; 32]),
                ..result
            },
        ];
        for other in others {
            assert!(!other.signed_by(&secp, &key, id, &signature), "{other:?}");
        }
    }
}
