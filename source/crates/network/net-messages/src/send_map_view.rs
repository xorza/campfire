use crate::send_entity_map::SendEntityMap;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity::{EntityMapper, hash_map::EntityHashMap};
use tracing::trace;

/// How a sender writes the entities a message names, as a read-only [`EntityMapper`].
///
/// Only a client maps, as in Replicon: it writes each entity as the server names it, and the
/// server sends its own entities. So no entity carries a flag that says which world names it.
///
/// Holds only shared references and performs only reads, so sharing this view across parallel
/// tasks is sound. `set_mapped` is unsupported, mirroring `bevy_replicon`'s send context: mapping
/// code that runs during serialization must not insert mappings.
#[derive(Debug, Clone, Copy)]
pub enum SendMapView<'a> {
    /// The receiver names entities as this world does, or checks them: a server's side of a
    /// link, or a host client.
    AsIs,
    /// A client's side: the server's entity from `shared` first, then from `local`. An entity
    /// that neither holds becomes [`Entity::PLACEHOLDER`], which the server cannot take for one
    /// of its own.
    Lookup {
        /// Replicon's map of the replicated entities.
        shared: Option<&'a EntityHashMap<Entity>>,
        /// The link's own pairs, which replicon never sees.
        local: &'a SendEntityMap,
    },
}

impl EntityMapper for SendMapView<'_> {
    fn get_mapped(&mut self, entity: Entity) -> Entity {
        let Self::Lookup { shared, local } = *self else {
            return entity;
        };
        let mapped = shared
            .and_then(|shared| shared.get(&entity))
            .or_else(|| local.0.get(&entity))
            .copied();
        trace!(
            target: "campfire_net_debug::entity",
            kind = "entity_map_send",
            direction = "send",
            source_entity = ?entity,
            remote_entity = ?mapped,
            "mapped entity while serializing"
        );
        mapped.unwrap_or(Entity::PLACEHOLDER)
    }

    fn set_mapped(&mut self, _source: Entity, _target: Entity) {
        panic!(
            "SendMapView is read-only; MapEntities impls used during serialization must not insert mappings"
        );
    }
}
