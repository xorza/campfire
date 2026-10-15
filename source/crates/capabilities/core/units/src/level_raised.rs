use std::num::NonZeroU32;

use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;

/// A unit's level rises to `level`: progression, which decides when it changes, triggers it,
/// and stats, which owns the level, observes it, so neither writes the other's state.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelRaised {
    pub entity: Entity,
    pub level: NonZeroU32,
}
