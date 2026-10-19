use bevy_ecs::prelude::*;
use campfire_net_core::NetId;

/// Maps replicon's usize channel indices to the net ids of the stack's channels.
///
/// Replicon has two channel namespaces, both starting from index 0: the server sends on
/// `server_channels` and receives on `client_channels`, and the client does the reverse. Each
/// replicon channel has one stack channel, which travels in that one direction.
#[derive(Debug, Resource)]
pub(crate) struct RepliconChannelMap {
    /// The stack channel of each replicon server channel index.
    pub server_channels: Vec<NetId>,
    /// The stack channel of each replicon client channel index.
    pub client_channels: Vec<NetId>,
}
