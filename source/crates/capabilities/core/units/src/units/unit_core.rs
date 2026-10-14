use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_declare::EntityKind;
use campfire_sim::{Position, StableId};
use campfire_values::{Team, UnitTags, UnitType};

use crate::units::body_unit::BodyUnit;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::owner::Owner;
use crate::units::spawn_target::SpawnTarget;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_states::UnitStates;

/// What every unit spawns with, a delivery among them: its stable id, place, type, team and
/// owner, and its script state; its type gives it its tags. Each spawn path spawns the unit with
/// its own parts, puts the core on, then the parts the modules declare for its kind, made from its
/// type's books, as the mode's spawner and the delivery spawn do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitCore {
    pub id: StableId,
    pub pos: Position,
    pub unit_type: UnitType,
    pub team: Team,
    pub owner: Option<Owner>,
}

/// A unit's core with what it derives from the books, made before the unit spawns, so a spawn
/// that holds the world reads no book while it puts the parts on.
#[derive(Debug)]
pub struct MadeCore {
    core: UnitCore,
    states: UnitStates,
}

impl UnitCore {
    /// The core's parts: its script state at `initial`, its type's defaults, but what a call
    /// wrote to it, which `new_states` holds for a unit that spawns after the call that created
    /// it.
    pub fn made(self, mut initial: UnitStates, new_states: &mut NewUnitStates) -> MadeCore {
        new_states.take(self.id, &mut initial);
        MadeCore {
            core: self,
            states: initial,
        }
    }

    /// Spawns the unit, a `BodyUnit`, in `world` with `parts`, which hold no part of the core,
    /// then puts its core on, made from the world's books.
    pub fn spawn(self, world: &mut World, parts: impl Bundle) -> EntityWorldMut<'_> {
        self.spawn_as(world, BodyUnit, parts)
    }

    /// Spawns the unit, of the kind `kind` marks, as `spawn` does.
    pub(crate) fn spawn_as<K: EntityKind>(
        self,
        world: &mut World,
        kind: K,
        parts: impl Bundle,
    ) -> EntityWorldMut<'_> {
        let owned = self.owner.is_some();
        let initial = world.resource::<UnitStateBook>().initial(self.unit_type);
        let made = self.made(initial, &mut world.resource_mut::<NewUnitStates>());
        let mut unit = world.spawn((kind, parts));
        debug_assert!(
            !(unit.contains::<StableId>()
                || unit.contains::<Position>()
                || unit.contains::<UnitType>()
                || unit.contains::<Team>()
                || unit.contains::<UnitTags>()
                || owned && unit.contains::<Owner>()),
            "a unit's parts hold none of its core, which would take their place"
        );
        made.insert_into(&mut unit);
        unit
    }
}

impl MadeCore {
    /// Puts the core on `unit`.
    pub fn insert_into(self, unit: &mut impl SpawnTarget) {
        let UnitCore {
            id,
            pos,
            unit_type,
            team,
            owner,
        } = self.core;
        unit.insert_part((id, pos, unit_type, team));
        if let Some(owner) = owner {
            unit.insert_part(owner);
        }
        self.states.insert_into(unit);
    }
}
