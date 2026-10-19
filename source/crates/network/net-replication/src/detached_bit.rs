use bevy_ecs::prelude::*;
use bevy_replicon::prelude::ScopeLifetime;
use bevy_replicon::server::visibility::filters_mask::FilterBit;
use bevy_replicon::server::visibility::registry::FilterRegistry;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;

/// The filter bit that hides every entity from a link that lost its
/// [`ReplicationSender`](crate::replication_sender::ReplicationSender).
///
/// Its lifetime is `WhileVisible`, so the link's client despawns what it held, and Replicon
/// spawns each entity anew once the sender comes back. Replicon has no pause: it resends no
/// update, so a link that skipped some would diverge.
#[derive(Debug, Resource, Clone, Copy)]
pub(crate) struct DetachedBit(pub(crate) FilterBit);

impl FromWorld for DetachedBit {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope(|world, mut filter_registry: Mut<'_, FilterRegistry>| {
            world.resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
                Self(filter_registry.register_scope::<Entity>(
                    world,
                    &mut registry,
                    ScopeLifetime::WhileVisible,
                ))
            })
        })
    }
}
