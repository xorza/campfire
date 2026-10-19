use bevy_ecs::prelude::*;
use core::fmt::Debug;

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
}
