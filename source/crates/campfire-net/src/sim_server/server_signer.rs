use bevy_ecs::resource::Resource;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SignOnly};
use campfire_protocol::{Checkpoint, Receipt, ServerInput, SessionId, SessionResult, Signature};
use campfire_runner::{ServerInputRefused, Session};

/// The server's key, which signs each input the server logs, each with fresh auxiliary
/// randomness.
#[derive(Resource, Debug)]
pub(crate) struct ServerSigner {
    key: Keypair,
    entropy: fn(&mut [u8; 32]),
    secp: Secp256k1<SignOnly>,
}

impl ServerSigner {
    pub(crate) fn new(key: Keypair, entropy: fn(&mut [u8; 32])) -> ServerSigner {
        ServerSigner {
            key,
            entropy,
            secp: Secp256k1::signing_only(),
        }
    }

    /// The server key's signature over the checkpoint `record` of the session of `session_id`.
    pub(crate) fn sign_checkpoint(&self, record: &Checkpoint, session_id: SessionId) -> Signature {
        let mut aux = [0; 32];
        (self.entropy)(&mut aux);
        record.sign(&self.secp, &self.key, session_id, &aux)
    }

    /// The server key's signature over the result `result` of the session of `session_id`.
    pub(crate) fn sign_result(&self, result: &SessionResult, session_id: SessionId) -> Signature {
        let mut aux = [0; 32];
        (self.entropy)(&mut aux);
        result.sign(&self.secp, &self.key, session_id, &aux)
    }

    /// The server key's signature over `receipt`.
    pub(crate) fn sign_receipt(&self, receipt: &Receipt) -> Signature {
        let mut aux = [0; 32];
        (self.entropy)(&mut aux);
        receipt.sign(&self.secp, &self.key, &aux)
    }

    /// Logs `input` in `session` before its next tick, signed at the place it takes; see
    /// `Session::serve`.
    pub(crate) fn serve(
        &self,
        session: &mut Session,
        input: ServerInput<'_>,
    ) -> Result<(), ServerInputRefused> {
        let mut aux = [0; 32];
        (self.entropy)(&mut aux);
        session.serve(input, &self.secp, &self.key, &aux)
    }
}
