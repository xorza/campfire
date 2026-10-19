//! Replication visibility and interest management.
//!
//! The stack provides entity-level helpers in `VisibilityExt` and room-based interest management in
//! [`room_plugin`](crate::room_plugin). For component-level visibility, register a Replicon
//! [`VisibilityFilter`] whose scope contains the components to gate. Add the filter component to
//! the replicated entity and its [`VisibilityFilter::ClientComponent`] to each sender link entity.
//! If both sides use the same component type, set `ClientComponent = Self`:
//!
//! ```rust,ignore
//! use bevy_ecs::prelude::*;
//! use bevy_replicon::prelude::{
//!     AppVisibilityExt, SingleComponent, VisibilityFilter,
//! };
//!
//! #[derive(Component, PartialEq)]
//! #[component(immutable)]
//! struct Team(u8);
//!
//! #[derive(Component)]
//! struct PrivateState(u32);
//!
//! impl VisibilityFilter for Team {
//!     type ClientComponent = Self;
//!     type Scope = SingleComponent<PrivateState>;
//!
//!     fn is_visible(
//!         &self,
//!         _client: Entity,
//!         client_team: Option<&Self::ClientComponent>,
//!     ) -> bool {
//!         client_team == Some(self)
//!     }
//! }
//!
//! // Register the filter and its component scope during app setup.
//! app.add_visibility_filter::<Team>();
//!
//! // `sender_link` is the entity representing this client connection.
//! commands.entity(sender_link).insert(Team(1));
//! commands.spawn((Replicate::default(), PrivateState(42), Team(1)));
//! ```
//!
//! Use a tuple such as `(Health, Inventory)` for `Scope` when several
//! components should share one visibility bit. The filter and client component
//! can also be different types when client permissions and entity metadata are
//! represented separately.
//!
//! [`VisibilityFilter`]: bevy_replicon::prelude::VisibilityFilter
//! [`VisibilityFilter::ClientComponent`]: bevy_replicon::prelude::VisibilityFilter::ClientComponent
//!
//! # Network visibility
//!
//! The **network visibility** is used to determine which entities are replicated to a client. The
//! server will only replicate the entities that are relevant to a client. When an entity loses
//! visibility, it can either be despawned on that client or retained without receiving updates.
//! This lets you save bandwidth by only sending the necessary data to each client.
//!
//! Replicon supports three visibility scope lifetimes:
//!
//! | Scope lifetime | Hidden before first replication | Hidden after replication |
//! | --- | --- | --- |
//! | [`WhileVisible`](bevy_replicon::prelude::ScopeLifetime::WhileVisible) | Not spawned | Despawned |
//! | [`AfterFirstVisibility`](bevy_replicon::prelude::ScopeLifetime::AfterFirstVisibility) | Not spawned | Retained without updates |
//! | [`AlwaysPresent`](bevy_replicon::prelude::ScopeLifetime::AlwaysPresent) | Spawned, but receives no further updates | Retained without updates |
//!
//! `VisibilityExt::lose_visibility` keeps its common despawning behavior. The two retaining
//! lifetimes are exposed separately because they differ before the first replication:
//!
//! - `VisibilityExt::lose_visibility_retained` retains an entity only after the client has seen it.
//!   This is useful for last-known-state views, stable client-side references, or avoiding repeated
//!   spawn setup when an entity moves in and out of interest.
//! - `VisibilityExt::lose_visibility_always_present` ensures that the entity is spawned even if it
//!   is already hidden, then pauses further updates. This is useful for identity or hierarchy roots,
//!   roster entries, objectives, and other placeholders that must exist before their live state is
//!   relevant. It deliberately reveals the entity's existence and initial state, so it should not be
//!   used when hidden entities must remain secret.
//!
//! The retaining policies pause mutations while hidden. The client keeps the last state it
//! received, and is not automatically notified that the entity became hidden. Applications that
//! need to mark the state as stale or suspend prediction should send or derive that state
//! separately. Replicon also does not replay component removals that occur while a retaining scope
//! is hidden, so retained entities should generally have a stable component layout or an
//! application-level structural resync.
//!
//! Visibility only controls what happens when an entity becomes hidden. Actually despawning an
//! authoritative entity still despawns every retained remote copy. To intentionally despawn only on
//! the sender, remove [`Replicated`] before despawning the entity.
//!
//! Visibility is cached, so after you set an entity as `visible` for a client, it will remain
//! relevant until you change the setting again.
//!
//! ```rust,no_run
//! # use bevy_ecs::entity::Entity;
//! # use bevy_ecs::prelude::World;
//! # use campfire_net_replication::VisibilityExt;
//!
//! # let client = Entity::from_bits(1);
//! # let entity = Entity::from_bits(2);
//! # let mut world = World::new();
//! world.gain_visibility(entity, client);
//! world.lose_visibility(entity, client);
//! ```

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
/// `AlwaysPresent` scope is hidden, because those scopes retain the remote entity when visibility
/// is lost. That is surprising for an actual server-side despawn: visibility loss should retain the
/// remote entity, but ending the authoritative entity's lifecycle should still remove it. Clearing
/// The stack's retaining bits here lets Replicon send the authoritative despawn.
///
/// If the server intentionally wants to despawn its local entity without sending a despawn to the
/// client, it should remove [`Replicated`] before despawning, so this observer will not run for
/// that entity.
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

/// Plugin that handles the visibility system
#[derive(Debug, Default)]
pub(crate) struct NetworkVisibilityPlugin;

impl Plugin for NetworkVisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisibilityBits>();
        app.add_observer(clear_retained_visibility_on_despawn);
        app.add_observer(apply_pending_visibility);
    }
}

#[cfg(test)]
mod tests;
