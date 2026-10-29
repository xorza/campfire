use bevy_ecs::prelude::*;

use crate::network_visibility_plugin::{VisibilityChange, VisibilityWrite};
use crate::visibility_bits::VisibilityBits;

/// Shows or hides a replicated entity for a link, at once on a [`World`] and deferred on
/// [`Commands`]; each hierarchy member follows the change unless a write of its own overrides it
/// (11-net-stack/replication.md, A member follows its root for each aspect, unless it overrides
/// that aspect).
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
