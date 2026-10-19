use bevy_ecs::prelude::*;

/// Marker component added to a link entity to enable incoming replication.
///
/// A link entity represents a connection to a remote peer. Adding
/// `ReplicationReceiver` to it allows the replication systems to process
/// entity data received through that connection.
///
/// Replication runs from the server to its clients, so a client's link needs it and a server's
/// [`ClientOf`](campfire_net_connection::ClientOf) links need
/// [`ReplicationSender`](crate::replication_sender::ReplicationSender).
///
/// Removing this component ends the receiver's current replication epoch and cleans up received
/// entities unless they are marked `Persistent`.
#[derive(Debug, Component, Default)]
pub struct ReplicationReceiver;
