use crate::controlled_send::ControlledSend;
use crate::lifetime::Lifetime;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use bevy_replicon::prelude::ScopeLifetime;
use bevy_replicon::prelude::SingleComponent;
use bevy_replicon::prelude::VisibilityFilter;
use campfire_net_connection::RemoteId;
use campfire_net_despawn::DespawnReplicated;
use tracing::trace;

/// Sender-side component that associates the entity with a
/// [`ReplicationSender`](crate::replication_sender::ReplicationSender) 'controlling' the entity
///
/// The receiver will add a `Controlled` marker component upon receiving the entity.
///
/// When the link goes, as the connection layer despawns a disconnected link, the sender will
/// optionally (based on the [`Lifetime`] value) despawn the entity. To keep a received entity alive
/// when its connection ends, use [`Persistent`](crate::persistent::Persistent) on the receiver side
/// instead.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Reflect)]
#[require(ControlledSend)]
#[reflect(Component)]
#[component(immutable)]
#[relationship(relationship_target = ControlledByRemote)]
pub struct ControlledBy {
    /// Which peer controls this entity? This should be an entity with a
    /// [`ReplicationSender`](crate::replication_sender::ReplicationSender) component.
    #[relationship]
    pub link: Entity,
    /// What happens to the entity on the sender-side if the controlling link disconnects?
    pub lifetime: Lifetime,
}

/// `ControlledSend` is receiver-local: only the controlling link sees it.
///
/// `RemoteId` is only the required client-component slot; the predicate uses
/// the link entity itself, so links without an id receive nothing.
impl VisibilityFilter for ControlledBy {
    type ClientComponent = RemoteId;
    type Scope = SingleComponent<ControlledSend>;
    const LIFETIME: ScopeLifetime = ScopeLifetime::WhileVisible;

    fn is_visible(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        remote.is_some_and(|_| link == self.link)
    }
}

impl ControlledBy {
    /// When the link goes, as the connection layer despawns a disconnected link, each entity it
    /// controlled for its session goes too.
    pub(crate) fn end_link_session(
        trigger: On<'_, '_, Despawn<ControlledByRemote>>,
        mut commands: Commands<'_, '_>,
        controlled_by_remote: Query<'_, '_, &ControlledByRemote>,
        controlled_by: Query<'_, '_, &Self>,
    ) {
        let Ok(owned) = controlled_by_remote.get(trigger.entity) else {
            return;
        };
        for entity in owned.collection() {
            if let Ok(owned_by) = controlled_by.get(*entity) {
                match owned_by.lifetime {
                    Lifetime::SessionBased => {
                        trace!(
                            "Despawning entity {entity:?} controlled by despawned link {:?}",
                            trigger.entity
                        );
                        commands.entity(*entity).queue_silenced(DespawnReplicated);
                    }
                    Lifetime::Independent => {}
                }
            }
        }
    }
}

/// Component on the sender side that lists the entities controlled by the remote peer
#[derive(Component, Clone, PartialEq, Eq, Debug, Reflect)]
#[relationship_target(relationship = ControlledBy)]
#[reflect(Component)]
pub struct ControlledByRemote(Vec<Entity>);
