use bevy_ecs::resource::Resource;
use campfire_protocol::ServerInput;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SignOnly};
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

    /// Logs `input` in `session` before its next tick, signed at the place it takes; see
    /// `Session::record_server`.
    pub(crate) fn serve(
        &self,
        session: &mut Session,
        input: ServerInput,
    ) -> Result<(), ServerInputRefused> {
        let log = session.log();
        let mut aux = [0; 32];
        (self.entropy)(&mut aux);
        let signature = input.sign(
            &self.secp,
            &self.key,
            log.session_id(),
            log.next_place(),
            &aux,
        );
        session.record_server(input, &signature)
    }
}
