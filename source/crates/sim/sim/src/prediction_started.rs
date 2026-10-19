use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;

/// A predicting client starts to predict the unit `entity`, whose parts replication wrote: as it
/// came predicted, or as its prediction began after the client held it unpredicted. Each cache
/// that follows the units the sim runs takes it (04-capabilities/00-overview.md, One due index for
/// each timed process).
#[derive(EntityEvent, Debug, Clone, Copy)]
pub struct PredictionStarted {
    pub entity: Entity,
}
