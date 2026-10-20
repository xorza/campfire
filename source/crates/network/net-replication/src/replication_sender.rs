use bevy_ecs::prelude::*;

/// The marker of a link that replicates: a connected link without it carries messages and no
/// replicated entity, and removing it detaches the link, whose client despawns what it held
/// (11-net-stack/connection.md, Removing `ReplicationSender` detaches a link, and re-adding it
/// replicates anew).
#[derive(Debug, Component, Default)]
pub struct ReplicationSender;
