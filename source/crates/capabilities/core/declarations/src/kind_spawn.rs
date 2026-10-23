use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use campfire_values::UnitType;

use crate::by_id::ById;
use crate::per_id::PerId;

/// An entity of the host `H` as its parts are made, at its spawn or as a restore checks it: the
/// world it is in, the entity, which holds what its spawn's caller gave it, and the core it
/// spawns with, which the same write as its parts puts on it; a restore reads the core from the
/// entity.
#[derive(Debug)]
pub struct KindSpawn<'w, H> {
    world: &'w World,
    entity: Entity,
    core: Option<SpawnCore>,
    host: PhantomData<H>,
}

/// The core a unit spawns with, as its parts read it: its place, its type, and the angle in
/// degrees it faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnCore {
    pub position: Vec3,
    pub unit_type: UnitType,
    pub angle: Num,
}

impl<'w, H> KindSpawn<'w, H> {
    /// `entity` of `world`, as it stands, its core its own.
    pub const fn of(world: &'w World, entity: Entity) -> Self {
        KindSpawn {
            world,
            entity,
            core: None,
            host: PhantomData,
        }
    }

    /// `entity` of `world`, which spawns with `core`, not on it yet; none for an entity of no
    /// core, as the match's or a player's.
    pub const fn spawning(world: &'w World, entity: Entity, core: Option<SpawnCore>) -> Self {
        KindSpawn {
            world,
            entity,
            core,
            host: PhantomData,
        }
    }

    pub const fn world(&self) -> &'w World {
        self.world
    }

    pub const fn entity(&self) -> Entity {
        self.entity
    }

    /// The unit's place, of its core; none for a restored unit that lacks it, which the core's
    /// check refuses.
    pub fn position(&self) -> Option<Vec3> {
        match self.core {
            Some(core) => Some(core.position),
            None => self
                .world
                .get::<Position>(self.entity)
                .map(|place| place.get()),
        }
    }

    /// The core it spawns with; none for a restored unit, whose parts are on it.
    pub const fn core(&self) -> Option<SpawnCore> {
        self.core
    }

    /// The unit's type, of its core; none for a restored unit that lacks it.
    pub fn unit_type(&self) -> Option<UnitType> {
        match self.core {
            Some(core) => Some(core.unit_type),
            None => self.world.get::<UnitType>(self.entity).copied(),
        }
    }

    /// What the book of `T` holds for the unit's type, of its core; none when the type has none,
    /// the match does not hold the book, or a restored unit lacks its type.
    pub fn of_type<T: PerId<UnitType>>(&self) -> Option<&'w T> {
        let book = self.world.get_resource::<ById<UnitType, T>>()?;
        book.get(self.unit_type()?)
    }
}
