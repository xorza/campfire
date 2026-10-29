use bevy_app::prelude::*;
use bevy_ecs::entity::{EntityHashMap, EntityHashSet};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryData;
use bevy_replicon::prelude::SyncRelatedAppExt;
use smallvec::SmallVec;
use tracing::trace;

use crate::disable_replicate_hierarchy::DisableReplicateHierarchy;
use crate::overridden::Overridden;
use crate::replicate_like::ReplicateLike;
use crate::replication_systems::ReplicationSystems;
use crate::replication_target::Replicate;

/// Attaches the `ChildOf` hierarchy of a replicated entity to it.
///
/// - When `Replicate` is added, every descendant gets a [`ReplicateLike`] that points at the
///   root, except a descendant with `DisableReplicateHierarchy` and its subtree. A descendant
///   with its own `Replicate` is attached too.
/// - An attached member follows the root's replication aspects (`Replicate`, the prediction and
///   interpolation targets, rooms and manual visibility) unless it overrides them with
///   [`Overridden`]. Each member holds its own copies, so the send path reads no root.
/// - A child that leaves its parent, or gains `DisableReplicateHierarchy`, leaves its root with
///   its subtree. The nearest member with a `Replicate` of its own becomes their root.
/// - A child that moves under a parent that gives no root leaves its root too, and one that loses
///   `DisableReplicateHierarchy` takes the root its parent gives, with its subtree.
#[derive(Debug, Default)]
pub(crate) struct HierarchySendPlugin;

/// The buffers of [`HierarchySendPlugin::link_hierarchy`], kept across its runs.
#[derive(Debug, Default)]
struct HierarchyWalk {
    /// Attachments queued this run. Inserts go through commands, so queries keep showing pre-flush
    /// state for the rest of the run: without this map a later walk would resolve through a stale
    /// attachment and queue a conflicting write (last-queued wins arbitrarily). Queued values are
    /// always fully resolved roots, so a single lookup suffices.
    pending: EntityHashMap<Entity>,
    /// The entities a walk this run reached.
    reached: EntityHashSet,
    origins: Vec<Origin>,
    stack: Vec<Entity>,
}

/// An entity a walk starts at: its depth below its top ancestor, and the root it attaches to.
#[derive(Debug, Clone, Copy)]
struct Origin {
    depth: usize,
    entity: Entity,
    attached_root: Option<Entity>,
}

/// What a hierarchy walk reads of one entity's attachment.
#[derive(Debug, QueryData)]
struct AttachState {
    replicate_like: Option<&'static ReplicateLike>,
    replicate: Has<Replicate>,
    replicate_overridden: Has<Overridden<Replicate>>,
    disabled: Has<DisableReplicateHierarchy>,
}

impl AttachStateItem<'_, '_> {
    /// Whether the entity's `Replicate` is its own, not a copy of its root's.
    const fn holds_replicate(&self) -> bool {
        self.replicate && (self.replicate_like.is_none() || self.replicate_overridden)
    }
}

impl Plugin for HierarchySendPlugin {
    fn build(&self, app: &mut App) {
        // Note: app.replicate::<ChildOf>() is called in SharedComponentRegistrationPlugin
        // so that FnsIds match between client and server.
        app.sync_related_entities::<ChildOf>();

        app.add_observer(Self::detach_on_replicate_removed);
        app.add_observer(Self::reattach_moved);
        app.add_observer(Self::reattach_disabled);
        app.add_observer(Self::reattach_enabled);
        app.add_systems(
            PostUpdate,
            Self::link_hierarchy.before(ReplicationSystems::Send),
        );
    }
}

impl HierarchySendPlugin {
    /// Attaches the descendants of each entity whose `Children` changed or that gained `Replicate`
    /// to its root: the walk starts at the changed entity, as its ancestors' attachments stand.
    ///
    /// A system, not an observer: an observer of `ChildOf` runs before the parent's `Children`
    /// shows the child.
    fn link_hierarchy(
        mut commands: Commands<'_, '_>,
        root_query: Query<
            '_,
            '_,
            (Entity, Option<&ReplicateLike>),
            (
                Or<(With<Replicate>, With<ReplicateLike>)>,
                Without<DisableReplicateHierarchy>,
                With<Children>,
                Or<(Changed<Children>, Added<Replicate>)>,
            ),
        >,
        children_query: Query<'_, '_, &Children>,
        parents: Query<'_, '_, &ChildOf>,
        // Children with their own `Replicate` are attached too: excluding them would freeze their
        // attachment on reparent and prune their descendants from the walk.
        child_likes: Query<'_, '_, Option<&ReplicateLike>, Without<DisableReplicateHierarchy>>,
        mut walk: Local<'_, HierarchyWalk>,
    ) {
        let HierarchyWalk {
            pending,
            reached,
            origins,
            stack,
        } = &mut *walk;
        pending.clear();
        reached.clear();
        origins.clear();
        // Walk the shallowest origins first: a nested origin walked before its ancestor would
        // attach its subtree to itself for one flush, and a child that joins it with an equal
        // value would then follow it instead of keeping the value as its own.
        origins.extend(root_query.iter().map(|(origin, like)| Origin {
            depth: Self::depth(origin, &parents),
            entity: origin,
            attached_root: like.map(|like| like.root),
        }));
        origins.sort_unstable_by_key(|origin| origin.depth);
        for origin in origins.iter() {
            // An ancestor's walk this run already attached the origin's whole subtree.
            if reached.contains(&origin.entity) {
                continue;
            }
            // If we are already ReplicateLike another entity, we use it as root
            let root = pending
                .get(&origin.entity)
                .copied()
                .or(origin.attached_root)
                .unwrap_or(origin.entity);

            // we go through all the descendants (instead of just the children) so that the root is
            // added and we don't need to search for the root ancestor in the replication systems
            stack.clear();
            stack.push(origin.entity);
            while let Some(parent) = stack.pop() {
                for child in children_query.relationship_sources(parent) {
                    let Ok(existing) = child_likes.get(child) else {
                        continue;
                    };
                    reached.insert(child);
                    // Effective attachment: this run's queued write wins over
                    // pre-flush query state (see `pending` above).
                    let current = pending
                        .get(&child)
                        .copied()
                        .or_else(|| existing.map(|like| like.root));
                    // Only write `ReplicateLike` when it is missing or points
                    // at a previous root: re-inserting the same value would
                    // pointlessly re-fire every `ReplicateLike` observer.
                    if current != Some(root) {
                        trace!("Adding ReplicateLike to child {child:?} with root {root:?}.");
                        commands.entity(child).insert(ReplicateLike { root });
                        pending.insert(child, root);
                    }
                    stack.push(child);
                }
            }
        }
    }

    fn depth(entity: Entity, parents: &Query<'_, '_, &ChildOf>) -> usize {
        let mut depth = 0;
        let mut current = entity;
        while let Ok(child_of) = parents.get(current) {
            depth += 1;
            current = child_of.parent();
        }
        depth
    }

    /// A root that stops replicating detaches its subtree, as a child that leaves it would be. A
    /// member's own `Replicate` is an override: its removal changes no attachment.
    fn detach_on_replicate_removed(
        trigger: On<'_, '_, Remove<Replicate>>,
        roots: Query<'_, '_, (), (Without<ReplicateLike>, Without<DisableReplicateHierarchy>)>,
        children: Query<'_, '_, &Children>,
        states: Query<'_, '_, AttachState>,
        mut commands: Commands<'_, '_>,
    ) {
        if !roots.contains(trigger.entity) {
            return;
        }
        for child in children.relationship_sources(trigger.entity) {
            Self::reattach_subtree(child, None, &children, &states, &mut commands);
        }
    }

    /// A child that leaves its parent, or moves under one that gives no root, leaves its root:
    /// it and its subtree are reattached as if the child were a root, so they follow the child's
    /// own `Replicate` or none. `Discard` fires as a parent goes, by a removal or by an insert that
    /// replaces it, which no `Remove` shows; the check runs once the change is in place. A parent
    /// that gives a root is the walk's: its `Children` change shows it the child.
    fn reattach_moved(trigger: On<'_, '_, Discard<ChildOf>>, mut commands: Commands<'_, '_>) {
        if trigger.trigger().new_archetype.is_some() {
            commands.run_system_cached_with(Self::detach_unless_rooted, trigger.entity);
        }
    }

    /// Detaches `entity` and its subtree from its root unless its parent gives one.
    fn detach_unless_rooted(
        In(entity): In<Entity>,
        parents: Query<'_, '_, &ChildOf>,
        children: Query<'_, '_, &Children>,
        states: Query<'_, '_, AttachState>,
        mut commands: Commands<'_, '_>,
    ) {
        let parent = parents.get(entity).map(ChildOf::parent);
        if parent.is_ok_and(|parent| Self::root_below(parent, &states).is_some()) {
            return;
        }
        Self::reattach_subtree(entity, None, &children, &states, &mut commands);
    }

    /// An entity that gains `DisableReplicateHierarchy` leaves its root with its subtree, as one
    /// that had it when it joined.
    fn reattach_disabled(
        trigger: On<'_, '_, Add<DisableReplicateHierarchy>>,
        children: Query<'_, '_, &Children>,
        states: Query<'_, '_, AttachState>,
        mut commands: Commands<'_, '_>,
    ) {
        Self::reattach_subtree(trigger.entity, None, &children, &states, &mut commands);
    }

    /// An entity that loses `DisableReplicateHierarchy` takes the root its parent gives, with its
    /// subtree, once the marker is gone: a `Remove` observer still sees it.
    fn reattach_enabled(
        trigger: On<'_, '_, Remove<DisableReplicateHierarchy>>,
        mut commands: Commands<'_, '_>,
    ) {
        if trigger.trigger().new_archetype.is_some() {
            commands.run_system_cached_with(Self::reattach_to_parent, trigger.entity);
        }
    }

    /// Attaches `entity` and its subtree to the root its parent gives, none without a parent.
    fn reattach_to_parent(
        In(entity): In<Entity>,
        parents: Query<'_, '_, &ChildOf>,
        children: Query<'_, '_, &Children>,
        states: Query<'_, '_, AttachState>,
        mut commands: Commands<'_, '_>,
    ) {
        let root = parents
            .get(entity)
            .ok()
            .and_then(|child_of| Self::root_below(child_of.parent(), &states));
        Self::reattach_subtree(entity, root, &children, &states, &mut commands);
    }

    /// The root the children of `parent` attach to: its root, or itself when its `Replicate` is
    /// its own; none below a `DisableReplicateHierarchy` entity or an entity that replicates not.
    fn root_below(parent: Entity, states: &Query<'_, '_, AttachState>) -> Option<Entity> {
        let state = states.get(parent).ok().filter(|state| !state.disabled)?;
        state
            .replicate_like
            .map(|like| like.root)
            .or_else(|| state.holds_replicate().then_some(parent))
    }

    /// Attaches `start` to `root`, or detaches it, and its descendants to what they follow then:
    /// the same root, or the nearest member with a `Replicate` of its own. A
    /// `DisableReplicateHierarchy` subtree stays detached.
    fn reattach_subtree(
        start: Entity,
        root: Option<Entity>,
        children: &Query<'_, '_, &Children>,
        states: &Query<'_, '_, AttachState>,
        commands: &mut Commands<'_, '_>,
    ) {
        let mut stack = SmallVec::<[(Entity, Option<Entity>); 8]>::new();
        stack.push((start, root));
        while let Some((entity, root)) = stack.pop() {
            let Ok(state) = states.get(entity) else {
                continue;
            };
            let attached = root.filter(|_| !state.disabled);
            let current = state.replicate_like.map(|like| like.root);
            match attached {
                Some(root) if current != Some(root) => {
                    commands.entity(entity).try_insert(ReplicateLike { root });
                }
                Some(_) => {}
                None if current.is_some() => {
                    commands.entity(entity).try_remove::<ReplicateLike>();
                }
                None => {}
            }
            let below = if state.disabled {
                None
            } else {
                attached.or_else(|| state.holds_replicate().then_some(entity))
            };
            for child in children.relationship_sources(entity) {
                stack.push((child, below));
            }
        }
    }
}
