use std::fmt;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Allow;
use bevy_ecs::system::{Query, Res, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::{StableId, Unpredicted};

use crate::units::engine_tag::EngineTag;
use crate::units::owner::Owner;
use crate::units::tag_book::TagBook;
use crate::units::unit_type::UnitType;

/// Who a player commands, the one rule the server's bots, a client and its view read: a player's
/// avatar, in a mode with avatars, is the unit its slot owns of a type the book tags `avatar`,
/// alive or dead, whichever capabilities the mode declares and however many units the player
/// owns. A world with no match, and so no tag book, has no avatar.
#[derive(SystemParam)]
pub struct PlayerUnits<'w, 's> {
    book: Option<Res<'w, TagBook>>,
    owned: Query<
        'w,
        's,
        (Entity, &'static StableId, &'static Owner, &'static UnitType),
        Allow<Unpredicted>,
    >,
}

/// `PlayerUnits` for code that holds the world itself, as an exclusive system does, its query
/// kept between reads.
pub struct HeldPlayerUnits(SystemState<PlayerUnits<'static, 'static>>);

/// A unit a player commands: its entity and its stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commanded {
    pub entity: Entity,
    pub id: StableId,
}

impl PlayerUnits<'_, '_> {
    /// The avatar of `slot`; the lowest stable id should it own several, none before it spawns
    /// or in a mode with no avatars.
    pub fn avatar(&self, slot: PlayerSlot) -> Option<Commanded> {
        let avatar = EngineTag::Avatar.tag();
        let book = self.book.as_deref()?;
        self.owned
            .iter()
            .filter(|&(_, _, owner, &unit_type)| {
                owner.slot() == slot && book.own(unit_type).contains(avatar)
            })
            .map(|(entity, &id, ..)| Commanded { entity, id })
            .min_by_key(|unit| unit.id)
    }
}

impl HeldPlayerUnits {
    pub fn new(world: &mut World) -> HeldPlayerUnits {
        HeldPlayerUnits(SystemState::new(world))
    }

    /// See `PlayerUnits::avatar`, of `world`, the world it was made of.
    pub fn avatar(&mut self, world: &World, slot: PlayerSlot) -> Option<Commanded> {
        let units = self.0.get(world).expect("its params are always valid");
        units.avatar(slot)
    }
}

/// Its state holds the world's ids of what it reads, which print nothing of use.
impl fmt::Debug for HeldPlayerUnits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HeldPlayerUnits")
    }
}

/// A system param holds borrows of the world, whose queries print nothing of use.
impl fmt::Debug for PlayerUnits<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlayerUnits")
    }
}
