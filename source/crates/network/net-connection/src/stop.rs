use bevy_ecs::prelude::*;
use campfire_net_payload::AppCode;

/// Stops a server.
#[derive(Debug, EntityEvent)]
pub struct Stop {
    pub entity: Entity,
    /// The code the stop reports.
    pub reason: AppCode,
}

impl From<Entity> for Stop {
    /// A stop with no reason given.
    fn from(entity: Entity) -> Self {
        Self {
            entity,
            reason: AppCode::ENDED,
        }
    }
}
