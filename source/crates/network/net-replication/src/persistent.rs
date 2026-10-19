use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;

/// Prevents receiver-side entities from being despawned when the connection ends or their
/// [`ReplicationReceiver`](crate::replication_receiver::ReplicationReceiver) is removed.
///
/// Add this marker to a received [`Replicated`](bevy_replicon::prelude::Replicated) entity to
/// preserve only that entity, or add it to the entity holding
/// [`ReplicationReceiver`](crate::replication_receiver::ReplicationReceiver) to preserve every
/// entity received through the receiver.
///
/// A kept entity becomes a local one: it loses its replication markers, as the receiver forgets
/// which sender entity it was, and a reconnect replicates the sender's entity anew, as a second
/// entity. This only affects disconnect cleanup. A despawn explicitly replicated by the sender
/// while connected still despawns the entity.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Persistent;
