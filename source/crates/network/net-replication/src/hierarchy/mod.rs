//! This module is responsible for making sure that parent-children hierarchies are replicated
//! correctly.
use crate::component_codec::ComponentCodec;
use crate::remote_parent::RemoteParent;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::RuleFns;
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::registry::ctx::{RemoveCtx, SerializeCtx, WriteCtx};
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use bytes::Bytes;

/// Serializes the server parent entity targeted by [`ChildOf`].
///
/// The stack registers a rule of its own for [`ChildOf`] so the receive path can
/// inspect the raw server entity before mapping it. If the parent is not mapped
/// yet, the receiver defers inserting the relationship instead of letting
/// Replicon create a placeholder entity inside the relationship component.
#[expect(
    clippy::unnecessary_wraps,
    reason = "Replicon's serialize fn returns a Result"
)]
pub(crate) fn serialize_child_of(
    _ctx: &mut SerializeCtx<'_>,
    child_of: &ChildOf,
    message: &mut Vec<u8>,
) -> Result<()> {
    ComponentCodec::write(&child_of.parent(), message);
    Ok(())
}

/// Deserializes the raw server parent entity for stale-message consumption.
///
/// The active receive path uses [`write_child_of`] so it can defer insertion
/// until the parent is mapped.
pub(crate) fn deserialize_child_of(
    _ctx: &mut WriteCtx<'_>,
    message: &mut Bytes,
) -> Result<ChildOf> {
    let server_parent = ComponentCodec::read(message)?;
    Ok(ChildOf(server_parent))
}

/// Receives [`ChildOf`] without using Replicon's placeholder entity mapper: the server's parent
/// is kept as [`RemoteParent`], and [`ChildOf`] is its view while the parent is mapped. An
/// unmapped parent waits for [`attach_remote_parents`].
pub(crate) fn write_child_of(
    ctx: &mut WriteCtx<'_>,
    _rule_fns: &RuleFns<ChildOf>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    let server_parent: Entity = ComponentCodec::read(message)?;
    entity.insert(RemoteParent::new(server_parent));
    if let Some(&client_parent) = ctx.entity_map.to_client().get(&server_parent) {
        entity.insert(ChildOf(client_parent));
    } else {
        entity.remove::<ChildOf>();
    }
    Ok(())
}

pub(crate) fn remove_child_of(_ctx: &mut RemoveCtx, entity: &mut DeferredEntity<'_>) {
    entity.remove::<ChildOf>();
    entity.remove::<RemoteParent>();
}

/// Gives each replicated entity whose server parent is mapped and that shows no [`ChildOf`]
/// its view: a parent that arrives late, or that comes back after it was despawned here. It runs
/// after the receive, and only in a frame whose receive mapped an entity.
pub(crate) fn attach_remote_parents(
    entity_map: Res<'_, ServerEntityMap>,
    detached: Query<'_, '_, (Entity, &RemoteParent), Without<ChildOf>>,
    mut commands: Commands<'_, '_>,
) {
    for (entity, parent) in &detached {
        if let Some(&client_parent) = entity_map.to_client().get(&parent.server) {
            commands.entity(entity).insert(ChildOf(client_parent));
        }
    }
}

#[cfg(test)]
mod tests;
