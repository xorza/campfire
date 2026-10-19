use bevy_ecs::prelude::*;

/// Marker component added to a link entity to enable outgoing replication.
///
/// A link entity represents a connection to a remote peer. Only connected links
/// with this marker are admitted into replication (via `ClientVisibility`);
/// links without it stay connected — messages still flow — but receive no
/// replicated entities or components.
///
/// Removing it from a link that replicates detaches the link: its client despawns every entity
/// it held, and inserting it again replicates each entity anew. The link's manual visibility
/// stays.
///
/// On the server, this is typically added in the `On<Add<LinkOf>>` observer:
///
/// ```rust,ignore
/// fn handle_new_client(trigger: On<Add<LinkOf>>, mut commands: Commands) {
///     commands.entity(trigger.entity).insert(ReplicationSender);
/// }
/// ```
#[derive(Debug, Component, Default)]
pub struct ReplicationSender;
