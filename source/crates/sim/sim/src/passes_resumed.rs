use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;

/// The unit `entity`, out of the world, comes back to the passes that skipped it, as a rider that
/// starts to fire from its container: each due index pushes it again, as it pushes a unit whose
/// `Disabled` goes (04-capabilities/interaction.md, A rider's timers wait and catch up).
#[derive(EntityEvent, Debug, Clone, Copy)]
pub struct PassesResumed {
    pub entity: Entity,
}
