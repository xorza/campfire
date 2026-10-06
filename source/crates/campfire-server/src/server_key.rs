use bevy_ecs::resource::Resource;
use campfire_protocol::secp256k1::Keypair;

/// The server's key, which signs what the server logs: the session's result among it.
#[derive(Resource, Debug)]
pub(crate) struct ServerKey(pub(crate) Keypair);
