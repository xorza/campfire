use bevy_ecs::prelude::*;
use smallvec::SmallVec;

/// Why ready networking entities could not be classified into a supported `NetworkTopology`.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum NetworkTopologyError {
    /// More than one client link is connected.
    #[error("multiple client links are connected: {0:?}")]
    MultipleConnectedClients(SmallVec<[Entity; 4]>),
    /// More than one server is started.
    #[error("multiple servers are started: {0:?}")]
    MultipleStartedServers(SmallVec<[Entity; 4]>),
    /// A client and a server are both ready in one app, which runs one side.
    #[error("connected client {client:?} and started server {server:?} share one app")]
    MixedClientServer {
        /// The connected client entity.
        client: Entity,
        /// The started server entity.
        server: Entity,
    },
}
