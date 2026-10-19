use bevy_ecs::prelude::*;

/// System set that refreshes the cached
/// [`NetworkTopology`](crate::network_topology::NetworkTopology).
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum NetworkTopologySystems {
    /// Infer the topology after networking lifecycle changes.
    Update,
}
