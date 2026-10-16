use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::stable_id::entity_index::EntityIndex;

/// Where a restored state breaks a rule that a module other than its type's owner checks: on one
/// entity, or in the match as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breach {
    Entity(Entity),
    Match,
}

impl Breach {
    /// `Match` when `holds` is false: a rule of the whole match.
    pub const fn of_match(holds: bool) -> Result<(), Self> {
        if holds { Ok(()) } else { Err(Self::Match) }
    }

    /// The first entity of `world`, by stable id, that `breaks` says breaks a rule a restored
    /// state keeps; none when none does.
    pub fn first(world: &World, breaks: impl Fn(Entity) -> bool) -> Result<(), Self> {
        let index = world.resource::<EntityIndex>();
        match index.iter().find(|&(_, entity)| breaks(entity)) {
            Some((_, entity)) => Err(Self::Entity(entity)),
            None => Ok(()),
        }
    }
}
