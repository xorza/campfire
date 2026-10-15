use bevy_ecs::resource::Resource;
use campfire_protocol::Signer;

/// The player's session key, which signs their join answer and every input message.
#[derive(Resource, Debug)]
pub(crate) struct PlayerSigner(pub(crate) Signer);
