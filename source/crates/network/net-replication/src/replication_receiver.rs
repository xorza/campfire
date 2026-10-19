use bevy_ecs::prelude::*;

/// The marker of a link that receives replication, a client's link; a server's links send with
/// [`ReplicationSender`](crate::replication_sender::ReplicationSender). Its removal ends the
/// receiver's replication epoch and despawns what it received but `Persistent` entities.
#[derive(Debug, Component, Default)]
pub struct ReplicationReceiver;
