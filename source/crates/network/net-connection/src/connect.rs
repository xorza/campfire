use bevy_ecs::prelude::*;

/// Connects a client.
#[derive(Debug, EntityEvent)]
pub struct Connect {
    pub entity: Entity,
}

impl From<Entity> for Connect {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}
