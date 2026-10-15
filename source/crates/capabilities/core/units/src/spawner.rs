use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_values::{NameId, UnitType};

use crate::facing::Facing;
use crate::spawn_at::SpawnAt;
use crate::spawn_hooks::SpawnHooks;
use crate::unit_entity::body_unit::BodyUnit;

/// How the match spawns a unit of a type the mode spawns, owned by a player or none: its core
/// with the parts the caller gives, and the parts the modules declare, then the spawn hooks of
/// the match's capabilities, a type's passive among them. `ModeUnits` builds it, as the types
/// are the mode's, and the mode, a capability that spawns a typed unit, such as production, and
/// a test call it. Package data, not state.
#[derive(Debug, Clone)]
pub struct Spawner {
    /// Whether it spawns each type, by type.
    types: Rc<[bool]>,
}

impl Spawner {
    /// The spawner of `types`.
    pub fn new(types: impl IntoIterator<Item = UnitType>) -> Spawner {
        let types: Vec<UnitType> = types.into_iter().collect();
        Spawner::of(&types)
    }

    fn of(types: &[UnitType]) -> Spawner {
        let len = types.iter().map(|unit_type| unit_type.index() + 1).max();
        let mut spawns = vec![false; len.unwrap_or(0)];
        for unit_type in types {
            spawns[unit_type.index()] = true;
        }
        Spawner {
            types: spawns.into(),
        }
    }

    /// Whether it spawns units of `unit_type`.
    pub fn spawns(&self, unit_type: UnitType) -> bool {
        self.types.get(unit_type.index()).copied().unwrap_or(false)
    }

    /// Spawns `at` in `world` with `parts`, facing its angle, and the parts the modules declare
    /// for a unit, but those of `parts`, then runs the capabilities' spawn hooks.
    pub fn spawn(&self, world: &mut World, at: SpawnAt, parts: impl Bundle) -> Entity {
        debug_assert!(self.spawns(at.unit_type), "a unit type the mode spawns");
        let entity = at
            .core()
            .spawn(world, BodyUnit, (Facing::of(at.angle), parts));
        SpawnHooks::run(world, entity);
        entity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spawner_spawns_exactly_its_types() {
        let spawner = Spawner::new([3, 1].map(UnitType::new));
        let spawns = [0, 1, 2, 3, 4].map(|index| spawner.spawns(UnitType::new(index)));
        assert_eq!(spawns, [false, true, false, true, false]);
        let none = Spawner::new([]);
        assert!(!none.spawns(UnitType::new(0)));
    }
}
