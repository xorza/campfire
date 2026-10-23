//! What the entity views of a link's sides read.
//!
//! Only a client maps, as in Replicon: through replicon's [`ServerEntityMap`] first, then the
//! link's own pairs in the [`LinkMessages`](crate::link_messages::LinkMessages), which
//! replicon never sees. A server keeps the entities it sends, and accepts from a client only one
//! it replicates or one of the link's own.

use crate::receive_entity_map::ReceiveEntityMap;
use crate::receive_map_view::ReceiveMapView;
use crate::send_entity_map::SendEntityMap;
use crate::send_map_view::SendMapView;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::system::{Query, Res, SystemParam};
use bevy_replicon::shared::replication::Replicated;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;

impl RepliconMapParam<'_, '_> {
    /// The send view of a client's side whose own pairs are `local`.
    pub(crate) fn send_view<'a>(&'a self, local: &'a SendEntityMap) -> SendMapView<'a> {
        SendMapView::Lookup {
            shared: self.map.as_deref().map(ServerEntityMap::to_server),
            local,
        }
    }

    /// The receive view of a client's side whose own pairs are `local`.
    pub(crate) fn receive_view<'a>(&'a self, local: &'a ReceiveEntityMap) -> ReceiveMapView<'a> {
        ReceiveMapView::Lookup {
            shared: self.map.as_deref().map(ServerEntityMap::to_client),
            local,
        }
    }

    /// The receive view of a server's side, of its link `link`.
    pub(crate) fn checked_view(&self, link: Entity) -> ReceiveMapView<'_> {
        ReceiveMapView::Checked {
            link,
            replicated: &self.replicated,
        }
    }
}

/// SystemParam exposing replicon's entity map, and what a server checks a client's entities by.
#[derive(SystemParam)]
pub(crate) struct RepliconMapParam<'w, 's> {
    /// Replicon's client-side entity map. Absent on servers and in apps without replication.
    map: Option<Res<'w, ServerEntityMap>>,
    replicated: Query<'w, 's, (), With<Replicated>>,
}

impl core::fmt::Debug for RepliconMapParam<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RepliconMapParam")
            .field("map", &self.map.is_some())
            .finish()
    }
}
