//! Each link's manual visibility of each replicated entity, on Replicon's visibility filters, and
//! the hierarchy members that follow their root's (11-net-stack/replication.md, A member follows
//! its root for each aspect, unless it overrides that aspect). Replicon does not replay a removal
//! made while a retaining scope hides the entity, so a retained entity keeps a stable layout.

use bevy_app::prelude::*;
use bevy_ecs::entity::EntityHashMap;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::Replicated;
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use bevy_replicon::server::visibility::filters_mask::FilterBit;

use crate::manual_visibility::ManualVisibility;
use crate::overridden::Overridden;
use crate::replicate_like::ReplicateLike;
use crate::replicate_like::ReplicateLikeChildren;
use crate::visibility_bits::VisibilityBits;

/// The manual visibility set for a link before replication admits it, which applies once the
/// link gets its [`ClientVisibility`]: the last state each entity was given.
#[derive(Component, Debug, Default)]
pub(crate) struct PendingVisibility {
    writes: EntityHashMap<Option<FilterBit>>,
}

/// Who writes a manual visibility state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VisibilityWrite {
    /// The user: on a hierarchy member, the write overrides the root's visibility.
    Direct,
    /// The hierarchy, from a root to the members that follow it.
    Hierarchy,
}

/// One manual visibility state for an entity on a link.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VisibilityChange {
    pub(crate) entity: Entity,
    pub(crate) link: Entity,
    /// The filter bit that hides the entity, or `None` to show it.
    pub(crate) hidden_bit: Option<FilterBit>,
    pub(crate) write: VisibilityWrite,
}

impl VisibilityChange {
    /// Sets this manual visibility state, and recursively through [`ReplicateLikeChildren`] on
    /// the members that follow the entity. A link that replication has not admitted yet keeps
    /// the state in its [`PendingVisibility`].
    pub(crate) fn apply(self, world: &mut World, bits: VisibilityBits) {
        let Self {
            entity,
            link,
            hidden_bit,
            write,
        } = self;
        let admitted = if let Some(mut visibility) = world.get_mut::<ClientVisibility>(link) {
            // The bits represent alternative states of one logical visibility filter. Clear every
            // previous state before selecting the new one so a shorter lifetime cannot remain
            // active.
            for bit in bits.iter() {
                visibility.set(entity, bit, true);
            }
            if let Some(bit) = hidden_bit {
                visibility.set(entity, bit, false);
            }
            true
        } else {
            false
        };
        if !admitted && let Ok(mut link_mut) = world.get_entity_mut(link) {
            link_mut
                .entry::<PendingVisibility>()
                .or_default()
                .get_mut()
                .writes
                .insert(entity, hidden_bit);
        }
        // The write API tells user writes from the hierarchy's, so a direct write on a member
        // overrides without a comparison across every link.
        if write == VisibilityWrite::Direct
            && world.get::<ReplicateLike>(entity).is_some()
            && let Ok(mut entity_mut) = world.get_entity_mut(entity)
        {
            entity_mut.insert(Overridden::<ManualVisibility>::default());
        }

        // A pending write reaches the members when it applies.
        if !admitted {
            return;
        }
        let Some(children) = world.get::<ReplicateLikeChildren>(entity) else {
            return;
        };
        // Copy the entity list to avoid borrowing world while we recurse.
        // ReplicateLikeChildren is typically very small (1-3 entities).
        let child_entities: smallvec::SmallVec<[Entity; 8]> = children.iter().collect();
        for child in child_entities {
            if world.get::<Overridden<ManualVisibility>>(child).is_none() {
                Self {
                    entity: child,
                    write: VisibilityWrite::Hierarchy,
                    ..self
                }
                .apply(world, bits);
            }
        }
    }
}

/// Each link's manual visibility.
#[derive(Debug, Default)]
pub(crate) struct NetworkVisibilityPlugin;

impl Plugin for NetworkVisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisibilityBits>();
        app.add_observer(Self::clear_retained_visibility_on_despawn);
        app.add_observer(Self::apply_pending_visibility);
    }
}

impl NetworkVisibilityPlugin {
    /// A link that replication admits applies the manual visibility it was given before.
    fn apply_pending_visibility(
        trigger: On<'_, '_, Add<ClientVisibility>>,
        pending: Query<'_, '_, (), With<PendingVisibility>>,
        mut commands: Commands<'_, '_>,
    ) {
        if !pending.contains(trigger.entity) {
            return;
        }
        let link = trigger.entity;
        commands.queue(move |world: &mut World| {
            let Ok(mut link_mut) = world.get_entity_mut(link) else {
                return;
            };
            let Some(pending) = link_mut.take::<PendingVisibility>() else {
                return;
            };
            let bits = *world.resource::<VisibilityBits>();
            for (entity, hidden_bit) in pending.writes {
                if world.get_entity(entity).is_ok() {
                    VisibilityChange {
                        entity,
                        link,
                        hidden_bit,
                        write: VisibilityWrite::Hierarchy,
                    }
                    .apply(world, bits);
                }
            }
        });
    }

    /// Makes a real authoritative despawn override the stack's retaining visibility scopes.
    ///
    /// Replicon normally suppresses an entity despawn while an `AfterFirstVisibility` or
    /// `AlwaysPresent` scope is hidden, because those scopes retain the remote entity when
    /// visibility is lost. That is surprising for an actual server-side despawn: visibility loss
    /// should retain the remote entity, but ending the authoritative entity's lifecycle should
    /// still remove it. Clearing The stack's retaining bits here lets Replicon send the
    /// authoritative despawn.
    ///
    /// If the server intentionally wants to despawn its local entity without sending a despawn to
    /// the client, it should remove [`Replicated`] before despawning, so this observer will not run
    /// for that entity.
    fn clear_retained_visibility_on_despawn(
        trigger: On<'_, '_, Despawn<Replicated>>,
        bits: Res<'_, VisibilityBits>,
        mut senders: Query<'_, '_, &mut ClientVisibility>,
    ) {
        for mut visibility in &mut senders {
            visibility.set(trigger.entity, bits.after_first_visibility, true);
            visibility.set(trigger.entity, bits.always_present, true);
        }
    }
}

#[cfg(test)]
mod tests;
