use bevy_ecs::prelude::*;
use campfire_net_payload::AppCode;

/// Trigger to disconnect a connection: a client, or a server's link to one.
#[derive(Debug, EntityEvent)]
pub struct Disconnect {
    pub entity: Entity,
    /// The code the disconnect reports, on this end and to the peer.
    pub reason: AppCode,
}

impl From<Entity> for Disconnect {
    /// A disconnect with no reason given.
    fn from(entity: Entity) -> Self {
        Self {
            entity,
            reason: AppCode::ENDED,
        }
    }
}
