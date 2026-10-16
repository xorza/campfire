use std::iter;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::{EntityKind, EntityParts, SpawnCore, SpawnParts};
use campfire_math::Vec3;
use campfire_sim::{Position, StableId};
use campfire_values::{Team, UnitTags, UnitType};

use crate::avatar::Avatar;
use crate::new_unit_states::NewUnitStates;
use crate::owner::Owner;
use crate::tag_book::TagBook;
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
    /// the core, then in one move its core, its owner, its script state, its mark as an avatar
    /// when its type's tags hold `avatar`, and the parts the modules declare for every unit and
    /// for its kind, but those of `parts`: two moves between archetypes. Its script state starts
    /// at its type's defaults, but what a call wrote to it before it spawned, which
    /// `NewUnitStates` holds.
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
        let avatar = world.get_resource::<TagBook>().is_some_and(|book| {
            let tags = book.unit_tags(unit_type, iter::empty());
            tags.tags.contains(Avatar::TAG.tag())
        });
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
        SpawnParts::give(&mut unit, |writer| {
            writer.push(id);
            writer.push(Position::from(pos));
            writer.push(unit_type);
            writer.push(team);
            if let Some(owner) = owner {
                writer.push(owner);
            }
            states.push_into(writer);
            if avatar {
                writer.push(Avatar);
            }
            let core = Some(SpawnCore { pos, unit_type });
            EntityParts::<UnitEntity>::push(writer, core);
            EntityParts::<K>::push(writer, core);
        });
        unit.id()
    }
}
