use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::{EntityWorldMut, Mut, World};
use campfire_sim::{Position, StableId};

use crate::units::body_unit::BodyUnit;
use crate::units::entity_kind::EntityKind;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::owner::Owner;
use crate::units::spawn_target::SpawnTarget;
use crate::units::team::Team;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_states::UnitStates;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

/// What every unit spawns with, a delivery among them: its stable id, place, type, team and
/// owner, and its script state; its type gives it its tags. Each spawn path spawns the unit with its own
/// parts, then puts the core on: a mode's unit its kit after, a delivery its spec before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitCore {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) unit_type: UnitType,
    pub(crate) team: Team,
    pub(crate) owner: Option<Owner>,
}

/// A unit's core with what it derives from the books, made before the unit spawns, so a spawn
/// that holds the world reads no book while it puts the parts on.
#[derive(Debug)]
pub(crate) struct MadeCore {
    core: UnitCore,
    states: UnitStates,
}

impl UnitCore {
    /// The core's parts: its script state at its type's defaults from `book`, but what a call
    /// wrote to it, which `new_states` holds for a unit that spawns after the call that created
    /// it.
    pub(crate) fn made(self, book: &UnitStateBook, new_states: &mut NewUnitStates) -> MadeCore {
        let mut written = book.initial(self.unit_type);
        new_states.take(self.id, &mut written);
        MadeCore {
            core: self,
            states: written,
        }
    }

    /// Spawns the unit, a `BodyUnit`, in `world` with `parts`, which hold no part of the core,
    /// then puts its core on, made from the world's books.
    pub(crate) fn spawn(self, world: &mut World, parts: impl Bundle) -> EntityWorldMut<'_> {
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
        let made = world.resource_scope(|world, mut new_states: Mut<'_, NewUnitStates>| {
            let states = world.resource::<UnitStateBook>();
            self.made(states, &mut new_states)
        });
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
    pub(crate) fn insert_into(self, unit: &mut impl SpawnTarget) {
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
