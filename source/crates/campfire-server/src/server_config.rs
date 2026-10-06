use bevy_ecs::resource::Resource;
use campfire_net::ServerSetup;

/// What the server opened or restored its session with: its key, which signs what it logs, its
/// certificate's hash, its times, its clock and its randomness.
#[derive(Resource, Debug)]
pub(crate) struct ServerConfig(pub(crate) ServerSetup);
