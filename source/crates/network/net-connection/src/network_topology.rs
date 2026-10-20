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
    /// The ready entities do not form one supported networking topology.
    Invalid(NetworkTopologyError),
}

impl NetworkTopology {
    /// Returns true for a connected client.
    pub const fn is_client(&self) -> bool {
        matches!(self, Self::Client(_))
    }
}
