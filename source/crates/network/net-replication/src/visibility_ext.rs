use crate::network_visibility_plugin::{VisibilityChange, VisibilityWrite};
use crate::visibility_bits::VisibilityBits;
use bevy_ecs::prelude::*;

/// Extension trait for dynamically showing or hiding replicated entities.
///
/// Implemented for both [`World`] (immediate) and [`Commands`] (deferred).
///
/// Descendant entities in the same replication hierarchy (those with [`ReplicateLikeChildren`])
/// follow each change, and so do descendants added to the hierarchy afterwards.
///
/// A hierarchy member with explicitly set visibility keeps it: the explicit write inserts
/// [`Overridden<ManualVisibility>`](crate::overridden::Overridden) and later root changes no longer
/// reach it. Remove the marker to follow the root's visibility again.
///
/// # Parameters
///
/// - `entity`: the replicated entity to show or hide.
/// - `link`: the link entity (connection) for which visibility changes.
///
/// # Example
///
/// ```rust,ignore
/// // Hide an entity from a specific client
/// commands.lose_visibility(server_entity, client_link_entity);
///
/// // Pause updates, retaining the entity if this client has already seen it
/// commands.lose_visibility_retained(server_entity, client_link_entity);
///
/// // Ensure the remote entity exists, but pause updates after its initial state
/// commands.lose_visibility_always_present(server_entity, client_link_entity);
///
/// // Make it visible again
/// commands.gain_visibility(server_entity, client_link_entity);
/// ```
///
/// [`ReplicateLikeChildren`]: crate::replicate_like::ReplicateLikeChildren
pub trait VisibilityExt {
    /// Make `entity` (and its replication-hierarchy descendants) visible to `link`.
    ///
    /// This clears every manual visibility constraint. Other constraints such as replication,
    /// prediction, interpolation, control, rooms, and user-defined visibility filters still apply.
    fn gain_visibility(&mut self, entity: Entity, link: Entity);

    /// Hide `entity` (and its replication-hierarchy descendants) from `link`, despawning the
    /// remote entity if it was previously replicated.
    fn lose_visibility(&mut self, entity: Entity, link: Entity);

    /// Hide `entity` (and its replication-hierarchy descendants) from `link` while retaining an
    /// existing remote entity without updates.
    ///
    /// If the client has never received the entity, it remains absent until visibility is gained.
    /// Despawning the authoritative entity still despawns its retained remote entity. Remove
    /// [`Replicate`](crate::replication_target::Replicate) first to intentionally suppress that
    /// despawn.
    fn lose_visibility_retained(&mut self, entity: Entity, link: Entity);

    /// Hide `entity` (and its replication-hierarchy descendants) from updates while ensuring that
    /// it is present on `link`.
    ///
    /// If the client has never received the entity, its initial state is still replicated. Further
    /// updates are paused until visibility is gained. Despawning the authoritative entity still
    /// despawns its retained remote entity. Remove
    /// [`Replicate`](crate::replication_target::Replicate) first to intentionally suppress that
    /// despawn.
    fn lose_visibility_always_present(&mut self, entity: Entity, link: Entity);
}

impl VisibilityExt for Commands<'_, '_> {
    fn gain_visibility(&mut self, entity: Entity, link: Entity) {
        self.queue(move |world: &mut World| {
            world.gain_visibility(entity, link);
        });
    }

    fn lose_visibility(&mut self, entity: Entity, link: Entity) {
        self.queue(move |world: &mut World| {
            world.lose_visibility(entity, link);
        });
    }

    fn lose_visibility_retained(&mut self, entity: Entity, link: Entity) {
        self.queue(move |world: &mut World| {
            world.lose_visibility_retained(entity, link);
        });
    }

    fn lose_visibility_always_present(&mut self, entity: Entity, link: Entity) {
        self.queue(move |world: &mut World| {
            world.lose_visibility_always_present(entity, link);
        });
    }
}

impl VisibilityExt for World {
    fn gain_visibility(&mut self, entity: Entity, link: Entity) {
        let bits = *self.resource::<VisibilityBits>();
        VisibilityChange {
            entity,
            link,
            hidden_bit: None,
            write: VisibilityWrite::Direct,
        }
        .apply(self, bits);
    }

    fn lose_visibility(&mut self, entity: Entity, link: Entity) {
        let bits = *self.resource::<VisibilityBits>();
        VisibilityChange {
            entity,
            link,
            hidden_bit: Some(bits.while_visible),
            write: VisibilityWrite::Direct,
        }
        .apply(self, bits);
    }

    fn lose_visibility_retained(&mut self, entity: Entity, link: Entity) {
        let bits = *self.resource::<VisibilityBits>();
        VisibilityChange {
            entity,
            link,
            hidden_bit: Some(bits.after_first_visibility),
            write: VisibilityWrite::Direct,
        }
        .apply(self, bits);
    }

    fn lose_visibility_always_present(&mut self, entity: Entity, link: Entity) {
        let bits = *self.resource::<VisibilityBits>();
        VisibilityChange {
            entity,
            link,
            hidden_bit: Some(bits.always_present),
            write: VisibilityWrite::Direct,
        }
        .apply(self, bits);
    }
}
