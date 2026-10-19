use bevy_ecs::prelude::*;

/// Trigger to start the server
#[derive(Debug, EntityEvent)]
pub struct Start {
    pub entity: Entity,
}

impl From<Entity> for Start {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}
