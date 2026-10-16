use bevy_ecs::resource::Resource;
use campfire_protocol::Signer;

/// The server's signer, of its key, which signs what its session logs and the receipts it
/// gives: only a server's world holds it, never a verifier's.
#[derive(Resource, Debug)]
pub(crate) struct ServerSigner(pub(crate) Signer);
