use campfire_codec::Binary;
use campfire_common::{StateHash, Tick};
use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::session_id::SessionId;
use crate::signature::Signature;
use crate::signer::Signer;
use crate::verifier::Verifier;

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
    /// The server key's signature over the result, `server`'s.
    pub fn sign(&self, server: &Signer, session_id: SessionId) -> Signature {
        let mut message = Vec::new();
        self.write_message(session_id, &mut message);
        server.sign(&message)
    }

    /// Whether `signature` is `server_key`'s over the result.
    pub(crate) fn signed_by(
        &self,
        verifier: &mut Verifier,
        server_key: &XOnlyPublicKey,
        session_id: SessionId,
        signature: &Signature,
    ) -> bool {
        verifier.verifies_written(*signature, server_key, |message| {
            self.write_message(session_id, message);
        })
    }

    /// Appends `domain ‖ session id ‖ postcard of the result` to `message`.
    fn write_message(&self, session_id: SessionId, message: &mut Vec<u8>) {
        message.extend_from_slice(SIGNATURE_DOMAIN);
        message.extend_from_slice(session_id.as_bytes());
        Binary::encode_into(self, &mut *message);
    }
}

#[cfg(test)]
mod tests;
