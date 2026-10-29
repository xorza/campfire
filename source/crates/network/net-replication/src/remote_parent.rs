use core::fmt::Debug;

use bevy_ecs::prelude::*;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;

/// The server's [`ChildOf`] of a replicated entity, as the server sends it: the server entity of
/// its parent. While the parent is mapped on this client, [`ChildOf`] is its view; while it is
/// not, as before the parent arrives or once the parent is despawned here, the entity has no
/// [`ChildOf`] and keeps this.
///
/// Replicon's default entity mapping creates a buffered placeholder for an entity that has not
/// appeared in the server-to-client map yet. That is unsafe for a relationship like
/// [`ChildOf`], whose hooks run at once and leave Replicon's placeholder buffer alive while the
/// next component in the same entity bundle is decoded.
#[derive(Debug, Component)]
pub(crate) struct RemoteParent {
    pub(crate) server: Entity,
}

impl RemoteParent {
    pub(crate) const fn new(server: Entity) -> Self {
        Self { server }
    }

    /// Gives each replicated entity whose server parent is mapped and that shows no [`ChildOf`] its
    /// view: a parent that arrives late, or that comes back after it was despawned here. It runs
    /// after each receive and visits the detached entities alone: Replicon inserts its map again
    /// in each frame it receives, so no change of the map tells a frame that mapped an entity.
    pub(crate) fn attach(
        entity_map: Res<'_, ServerEntityMap>,
        detached: Query<'_, '_, (Entity, &Self), Without<ChildOf>>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, parent) in &detached {
            if let Some(&client_parent) = entity_map.to_client().get(&parent.server) {
                commands.entity(entity).insert(ChildOf(client_parent));
            }
        }
    }
}
