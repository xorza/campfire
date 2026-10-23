use std::iter;

use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::{EntityKind, EntityParts, SpawnCore, SpawnParts};
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};
use campfire_values::{Team, UnitType};

use crate::avatar::Avatar;
use crate::new_unit_states::NewUnitStates;
use crate::owner::Owner;
use crate::tag_book::TagBook;
use crate::unit_entity::UnitEntity;
use crate::unit_state_book::UnitStateBook;
use crate::unit_states::UnitStates;
use crate::variation::Variation;

/// What every unit spawns with, a delivery among them: its stable id, place, type, team and
/// owner, the angle in degrees it faces, and its script state; its type gives it its tags, and
/// its variation when the type varies. Every spawn of a unit goes through `UnitCore::spawn`, the
/// mode's spawner's and a delivery's alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitCore {
    pub id: StableId,
    pub position: Vec3,
    pub unit_type: UnitType,
    pub team: Team,
    pub owner: Option<Owner>,
    pub angle: Num,
}

impl UnitCore {
    /// Spawns the unit, of the kind `kind` marks, in `world`, in one move between archetypes:
    /// with the kind's marker, `parts`, which hold no part of the core, its core, its owner, its
    /// script state, its mark as an avatar when its type's tags hold `avatar`, and the parts the
    /// modules declare for every unit and for its kind, but those of `parts`. Its script state
    /// starts at its type's defaults, but what a call wrote to it before it spawned, which
    /// `NewUnitStates` holds (02-engine-core.md, A unit spawns in one write).
    pub fn spawn<K: EntityKind, B: Bundle<Effect: NoBundleEffect>>(
        self,
        world: &mut World,
        kind: K,
        parts: B,
    ) -> Entity {
        let mut states = UnitStates::initial(world.resource::<UnitStateBook>(), self.unit_type);
        world
            .resource_mut::<NewUnitStates>()
            .take(self.id, &mut states);
        let Self {
            id,
            position,
            unit_type,
            team,
            owner,
            angle,
        } = self;
        let avatar = world.get_resource::<TagBook>().is_some_and(|book| {
            let tags = book.unit_tags(unit_type, iter::empty());
            tags.tags.contains(Avatar::TAG.tag())
        });
        // Drawn once, as its stream opens once for its unit in a tick, before the write, which
        // gives each part the one draw.
        let variation = Variation::draw(world, id, unit_type);
        // The write takes the parts by their components, which it finds registered.
        world.register_bundle::<(K, B)>();
        let mut unit = world.spawn_empty();
        SpawnParts::give(&mut unit, |writer| {
            // A part of `parts` that is one of the core's is pushed twice, which the writer
            // refuses.
            writer.push(kind);
            writer.push_bundle(parts);
            writer.push(id);
            writer.push(Position::from(position));
            writer.push(unit_type);
            writer.push(team);
            if let Some(owner) = owner {
                writer.push(owner);
            }
            states.push_into(writer);
            if avatar {
                writer.push(Avatar);
            }
            let core = Some(SpawnCore {
                position,
                unit_type,
                angle,
                variation: variation.map(Variation::get),
            });
            EntityParts::<UnitEntity>::push(writer, core);
            EntityParts::<K>::push(writer, core);
        });
        unit.id()
    }
}
