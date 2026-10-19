//! Server-side link relationships.
//!
//! A server entity can own many per-client link entities. This module models that fan-out with
//! Bevy's relationship API: [`Server`] is the relationship target, and [`LinkOf`] is inserted on
//! each child link entity to point back to the server. Transport crates can use this to keep the
//! server endpoint independent from the concrete links used for each connected peer.

use crate::link_state::Linked;
use crate::link_state::Linking;
use crate::link_state::Unlinked;
use crate::unlink::Unlink;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::relationship::Relationship;
use bevy_ecs::relationship::RelationshipHookMode;
use bevy_ecs::relationship::RelationshipSourceCollection;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use campfire_net_payload::UnlinkReason;
use tracing::trace;

/// Relationship target for a server endpoint that owns multiple link entities.
///
/// `Server` is inserted on the entity that represents the listening or hosting endpoint. Entities
/// with [`LinkOf`] are collected under this component, allowing systems to find and unlink all
/// child links when the server unlinks.
/// The target collection uses `linked_spawn`, so spawning a link with [`LinkOf`] can establish
/// the relationship at spawn time.
#[derive(Component, Default, Debug, Reflect)]
#[component(on_add = Server::on_add)]
#[relationship_target(relationship = LinkOf, linked_spawn)]
pub struct Server {
    #[relationship]
    links: Vec<Entity>,
}

impl Server {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity_ref = world.entity(context.entity);
        if !entity_ref.contains::<Unlinked>()
            && !entity_ref.contains::<Linked>()
            && !entity_ref.contains::<Linking>()
        {
            trace!("Inserting Unlinked because Server was added");
            world.commands().entity(context.entity).insert(Unlinked {
                reason: UnlinkReason::Initial,
            });
        }
    }

    /// An unlinked server unlinks each of its links: they end with it. The connection layer
    /// despawns them, so that each goes through its own disconnect first.
    pub(crate) fn unlinked(
        trigger: On<'_, '_, Add<Unlinked>>,
        query: Query<'_, '_, &Self, With<Unlinked>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(server_link) = query.get(trigger.entity) {
            for link_of in server_link.collection() {
                commands.trigger(Unlink {
                    entity: *link_of,
                    reason: UnlinkReason::ServerStopped,
                });
            }
        }
    }
}

/// Relationship source component for a link that belongs to a [`Server`].
///
/// Insert this on a per-client link entity and set [`server`](Self::server) to the server endpoint
/// entity.
/// Bevy's relationship hook adds each link to the [`Server`] collection; the custom removal hook
/// takes it out without removing the `Server` component when its last link goes.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Reflect)]
#[component(on_insert = LinkOf::on_insert_hook)]
#[component(on_discard = LinkOf::on_discard)]
pub struct LinkOf {
    /// Server endpoint that owns this link entity.
    pub server: Entity,
}

impl Relationship for LinkOf {
    type RelationshipTarget = Server;
    #[inline(always)]
    fn get(&self) -> Entity {
        self.server
    }
    #[inline]
    fn from(entity: Entity) -> Self {
        Self { server: entity }
    }

    fn set_risky(&mut self, entity: Entity) {
        self.server = entity;
    }
}

impl LinkOf {
    fn on_insert_hook(world: DeferredWorld<'_>, context: HookContext) {
        <Self as Relationship>::on_insert(world, context);
    }

    fn on_discard(
        mut world: DeferredWorld<'_>,
        HookContext {
            entity,
            relationship_hook_mode,
            ..
        }: HookContext,
    ) {
        match relationship_hook_mode {
            RelationshipHookMode::Run => {}
            RelationshipHookMode::Skip => return,
            RelationshipHookMode::RunIfNotLinked => {
                if <Server as RelationshipTarget>::LINKED_SPAWN {
                    return;
                }
            }
        }
        let target_entity = world.entity(entity).get::<Self>().unwrap().get();
        if let Ok(mut target_entity_mut) = world.get_entity_mut(target_entity)
            && let Some(mut relationship_target) = target_entity_mut.get_mut::<Server>()
        {
            RelationshipSourceCollection::remove(
                relationship_target.collection_mut_risky(),
                entity,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::Link;
    use crate::link_plugin::LinkPlugin;
    use crate::server_link_plugin::ServerLinkPlugin;
    use bevy_app::App;

    #[derive(Debug, Resource, Default)]
    struct UnlinkedChildren(Vec<Entity>);

    fn record_unlink(trigger: On<'_, '_, Unlink>, mut unlinked: ResMut<'_, UnlinkedChildren>) {
        unlinked.0.push(trigger.entity);
    }

    #[test]
    fn server_unlinked_triggers_unlink_for_child_links() {
        let mut app = App::new();
        app.add_plugins((LinkPlugin, ServerLinkPlugin));
        app.init_resource::<UnlinkedChildren>();
        app.add_observer(record_unlink);

        let server = app.world_mut().spawn((Server::default(), Linked)).id();
        let child = app
            .world_mut()
            .spawn((LinkOf { server }, Link::default(), Linked))
            .id();

        app.world_mut().entity_mut(server).insert(Unlinked {
            reason: UnlinkReason::TimedOut,
        });
        app.update();

        let unlinked = &app.world().resource::<UnlinkedChildren>().0;
        assert_eq!(unlinked, &[child]);
        // The child reports its server's stop, not the server's own reason, and stays for the
        // connection layer to end.
        assert_eq!(
            app.world()
                .get::<Unlinked>(child)
                .map(|unlinked| unlinked.reason),
            Some(UnlinkReason::ServerStopped)
        );
    }
}
