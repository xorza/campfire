use crate::network_topology_error::NetworkTopologyError;
use bevy_ecs::prelude::*;

/// The ready network side of this Bevy application.
///
/// Clients are included after they are `Connected` and servers after they are `Started`.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum NetworkTopology {
    /// No networking topology has been identified yet.
    #[default]
    Undefined,
    /// A connected client and its server link.
    Client(Entity),
    /// A started server.
    Server(Entity),
    /// A connected in-process client and its started server.
    HostClient {
        /// The started server entity.
        server: Entity,
        /// The connected host-client link entity.
        client: Entity,
    },
    /// The ready entities do not form one supported networking topology.
    Invalid(NetworkTopologyError),
}

impl NetworkTopology {
    /// Returns true for a connected client.
    pub const fn is_client(&self) -> bool {
        matches!(self, Self::Client(_))
    }

    /// Returns true for a started server, including the server side of a host-client app.
    pub const fn is_server(&self) -> bool {
        matches!(self, Self::Server(_) | Self::HostClient { .. })
    }

    /// Returns true for a started server without a connected in-process client.
    pub const fn is_headless_server(&self) -> bool {
        matches!(self, Self::Server(_))
    }

    /// Returns true for a ready in-process host-client app.
    pub const fn is_host_server(&self) -> bool {
        matches!(self, Self::HostClient { .. })
    }
}
