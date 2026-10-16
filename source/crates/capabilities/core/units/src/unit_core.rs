use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::{EntityKind, EntityParts, SpawnParts};
use campfire_math::Vec3;
use campfire_sim::{Position, StableId};
use campfire_values::{Team, UnitTags, UnitType};

use crate::new_unit_states::NewUnitStates;
use crate::owner::Owner;
use crate::unit_entity::UnitEntity;
use crate::unit_state_book::UnitStateBook;
use crate::unit_states::UnitStates;

/// What every unit spawns with, a delivery among them: its stable id, place, type, team and
/// owner, and its script state; its type gives it its tags. Every spawn of a unit goes through
/// `UnitCore::spawn`, the mode's spawner's and a delivery's alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitCore {
    pub id: StableId,
    pub pos: Vec3,
    pub unit_type: UnitType,
    pub team: Team,
    pub owner: Option<Owner>,
}

impl UnitCore {
    /// Spawns the unit, of the kind `kind` marks, in `world` with `parts`, which hold no part of
    /// the core: its core, then, in one move, the parts the modules declare for every unit and
    /// for its kind, but those of `parts`. Its script state starts at its type's defaults, but
    /// what a call wrote to it before it spawned, which `NewUnitStates` holds.
    pub fn spawn<K: EntityKind>(self, world: &mut World, kind: K, parts: impl Bundle) -> Entity {
        let mut states = UnitStates::initial(world.resource::<UnitStateBook>(), self.unit_type);
        world
            .resource_mut::<NewUnitStates>()
            .take(self.id, &mut states);
        let Self {
            id,
            pos,
            unit_type,
            team,
            owner,
        } = self;
        let mut unit = world.spawn((kind, parts));
        debug_assert!(
            !(unit.contains::<StableId>()
                || unit.contains::<Position>()
                || unit.contains::<UnitType>()
                || unit.contains::<Team>()
                || unit.contains::<UnitTags>()
                || owner.is_some() && unit.contains::<Owner>()),
            "a unit's parts hold none of its core, which would take their place"
        );
        unit.insert((id, Position::from(pos), unit_type, team));
        if let Some(owner) = owner {
            unit.insert(owner);
        }
        states.insert_into(&mut unit);
        SpawnParts::give(&mut unit, |world, entity, writer| {
            EntityParts::<UnitEntity>::push(world, entity, writer);
            EntityParts::<K>::push(world, entity, writer);
        });
        unit.id()
    }
}
