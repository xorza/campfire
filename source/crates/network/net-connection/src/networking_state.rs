use crate::network_topology::NetworkTopology;
use crate::remote_id::RemoteId;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_platform::collections::HashMap;
use campfire_net_core::PeerId;

/// The networking topology of this Bevy application, and the link entity of each remote peer.
///
/// `ConnectionPlugin` maintains this resource from role and lifecycle components. Users
/// can read [`topology`](Self::topology) and [`peer_map`](Self::peer_map), but do not need to
/// update them themselves.
#[derive(Resource, Debug, Clone)]
pub struct NetworkingState {
    /// The currently identified networking topology.
    pub topology: NetworkTopology,
    /// Mapping from remote peer IDs to their local connection entities.
    pub peer_map: HashMap<PeerId, Entity>,
    // This is mutated without triggering Bevy change detection. Consumers therefore only observe
    // a change after `topology` itself changes.
    pub(crate) dirty: bool,
}

impl NetworkingState {
    /// Removes `peer`'s mapping when it names `entity`: a reconnect maps the peer to its new
    /// entity before the old one goes.
    pub(crate) fn unmap(&mut self, peer: PeerId, entity: Entity) {
        if self.peer_map.get(&peer) == Some(&entity) {
            self.peer_map.remove(&peer);
        }
    }
}

impl Default for NetworkingState {
    fn default() -> Self {
        Self {
            topology: NetworkTopology::Undefined,
            peer_map: HashMap::default(),
            dirty: true,
        }
    }
}

/// Removes `entity`'s own `peer_map` entry, at its disconnect and again when `Connected` goes,
/// which a despawn without a disconnect also does.
pub(crate) fn unmap_remote(world: &mut DeferredWorld<'_>, entity: Entity) {
    let Some(peer) = world.get::<RemoteId>(entity).map(|remote| remote.0) else {
        return;
    };
    if let Some(mut metadata) = world.get_resource_mut::<NetworkingState>() {
        metadata.unmap(peer, entity);
    }
}
