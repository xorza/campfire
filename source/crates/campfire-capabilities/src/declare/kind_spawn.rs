use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::Position;

use crate::declare::per_type::PerType;
use crate::units::by_type::ByType;
use crate::units::facing::Facing;
use crate::units::unit_type::UnitType;

/// An entity of the host `H` as its parts are made, at its spawn or as a restore checks it: the
/// world it is in, and the entity, which holds its core and what its spawn's caller gave it.
#[derive(Debug)]
pub(crate) struct KindSpawn<'w, H> {
    world: &'w World,
    entity: Entity,
    host: PhantomData<H>,
}

impl<'w, H> KindSpawn<'w, H> {
    /// `entity` of `world`, as it stands.
    pub(crate) const fn of(world: &'w World, entity: Entity) -> KindSpawn<'w, H> {
        KindSpawn {
            world,
            entity,
            host: PhantomData,
        }
    }

    pub(crate) const fn world(&self) -> &'w World {
        self.world
    }

    pub(crate) const fn entity(&self) -> Entity {
        self.entity
    }

    /// The angle in degrees the unit faces, which its spawn gives; 0 for an entity that faces
    /// none.
    pub(crate) fn angle(&self) -> Num {
        let facing = self.world.get::<Facing>(self.entity);
        facing.map_or(Num::ZERO, |facing| facing.degrees())
    }

    /// The unit's place, which its core holds; none for a restored unit that lacks it, which
    /// the core's check refuses.
    pub(crate) fn pos(&self) -> Option<Position> {
        self.world.get::<Position>(self.entity).copied()
    }

    /// What the book of `T` holds for the unit's type, which its core holds; none when the type
    /// has none, the match does not hold the book, or a restored unit lacks its type.
    pub(crate) fn of_type<T: PerType>(&self) -> Option<&'w T> {
        let book = self.world.get_resource::<ByType<T>>()?;
        book.get(*self.world.get::<UnitType>(self.entity)?)
    }
}
