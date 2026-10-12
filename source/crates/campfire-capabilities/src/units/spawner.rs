use std::fmt;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::declare::entity_parts::EntityParts;
use crate::units::body_unit::BodyUnit;
use crate::units::facing::Facing;
use crate::units::spawn_at::SpawnAt;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_type::UnitType;

/// How the match spawns a unit of a type the mode spawns, owned by a player or none: its core
/// with the parts the caller gives, and the parts the modules declare, then what the mode does
/// for its type. `ModeUnits` builds it, as the types are the mode's, and the mode, a capability
/// that spawns a typed unit, such as production, and a test call it. Package data, not state.
#[derive(Clone)]
pub(crate) struct Spawner {
    /// Whether it spawns each type, by type.
    types: Rc<[bool]>,
    after: Rc<AfterFn>,
}

type AfterFn = dyn Fn(&mut World, Entity);

impl Spawner {
    /// The spawner of `types` that runs `after` on each unit it spawned.
    pub(crate) fn new(
        types: impl IntoIterator<Item = UnitType>,
        after: impl Fn(&mut World, Entity) + 'static,
    ) -> Spawner {
        let mut spawns = Vec::new();
        for unit_type in types {
            let at = unit_type.index();
            if spawns.len() <= at {
                spawns.resize(at + 1, false);
            }
            spawns[at] = true;
        }
        Spawner {
            types: spawns.into(),
            after: Rc::new(after),
        }
    }

    /// Whether it spawns units of `unit_type`.
    pub(crate) fn spawns(&self, unit_type: UnitType) -> bool {
        self.types.get(unit_type.index()).copied().unwrap_or(false)
    }

    /// Spawns `at` in `world` with `parts`, facing its angle, and the parts the modules declare
    /// for a unit, but those of `parts`, then runs what the mode does for its type.
    pub(crate) fn spawn(&self, world: &mut World, at: SpawnAt, parts: impl Bundle) -> Entity {
        debug_assert!(self.spawns(at.unit_type), "a unit type the mode spawns");
        let mut unit = at.core().spawn(world, (Facing::of(at.angle), parts));
        EntityParts::<UnitEntity>::give(&mut unit);
        EntityParts::<BodyUnit>::give(&mut unit);
        let entity = unit.id();
        (self.after)(world, entity);
        entity
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Spawner")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spawner_spawns_exactly_its_types() {
        let spawner = Spawner::new([3, 1].map(UnitType::new), |_, _| {});
        let spawns = [0, 1, 2, 3, 4].map(|index| spawner.spawns(UnitType::new(index)));
        assert_eq!(spawns, [false, true, false, true, false]);
        let none = Spawner::new([], |_, _| {});
        assert!(!none.spawns(UnitType::new(0)));
    }
}
