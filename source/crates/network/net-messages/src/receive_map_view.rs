use crate::entity_check::EntityCheck;
use crate::receive_entity_map::ReceiveEntityMap;
use crate::send_entity_map::SendEntityMap;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity::{EntityMapper, hash_map::EntityHashMap};
use tracing::trace;

/// How a receiver reads the entities a message names, as a read-only [`EntityMapper`].
///
/// The receive side of [`SendMapView`](crate::SendMapView): only a client maps, and a server
/// checks what a client names.
///
/// Holds only shared references and performs only reads, so sharing this view across parallel
/// tasks is sound. `set_mapped` is unsupported, mirroring `bevy_replicon`'s send context: mapping
/// code that runs during deserialization must not insert mappings.
#[derive(Debug, Clone, Copy)]
pub enum ReceiveMapView<'a> {
    /// A client's side: the local entity from `shared` first, then from `local`. An entity that
    /// neither holds becomes [`Entity::PLACEHOLDER`].
    Lookup {
        /// Replicon's map of the replicated entities.
        shared: Option<&'a EntityHashMap<Entity>>,
        /// The link's own pairs, which replicon never sees.
        local: &'a ReceiveEntityMap,
    },
    /// A server's side: an entity that `replicated` holds, or one of the link's own, as the
    /// client mapped it. Any other becomes [`Entity::PLACEHOLDER`], as an honest client names an
    /// entity that was hidden or despawned while its message was in flight.
    Checked {
        /// The link's own pairs, by the server's entity.
        known: &'a SendEntityMap,
        /// The entities the server replicates.
        replicated: &'a dyn EntityCheck,
    },
}

impl EntityMapper for ReceiveMapView<'_> {
    fn get_mapped(&mut self, entity: Entity) -> Entity {
        let mapped = match *self {
            Self::Lookup { shared, local } => shared
                .and_then(|shared| shared.get(&entity))
                .or_else(|| local.0.get(&entity))
                .copied(),
            Self::Checked { known, replicated } => {
                (known.0.contains_key(&entity) || replicated.holds(entity)).then_some(entity)
            }
        };
        trace!(
            target: "campfire_net_debug::entity",
            kind = "entity_map_receive",
            direction = "receive",
            remote_entity = ?entity,
            entity = ?mapped,
            "mapped entity while deserializing"
        );
        mapped.unwrap_or(Entity::PLACEHOLDER)
    }

    fn set_mapped(&mut self, _source: Entity, _target: Entity) {
        panic!(
            "ReceiveMapView is read-only; MapEntities impls used during deserialization must not insert mappings"
        );
    }
}
