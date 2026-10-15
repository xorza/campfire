use std::fmt;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, With, Without};
use bevy_ecs::system::{Query, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::{StableId, Unpredicted};
use campfire_values::UnitType;

use crate::avatar::Avatar;
use crate::dead::Dead;
use crate::owner::Owner;

/// Who a player commands, the one rule the server's bots, a client and its view read: a player's
/// avatar, in a mode with avatars, is the unit its slot owns of a type the book tags `avatar`,
/// alive or dead, whichever capabilities the mode declares and however many units the player
/// owns, as its type marks it an `Avatar`; and its living units of a type, which a bot's order
/// may name. A world with no match, and so no tag book, has no avatar.
#[derive(SystemParam)]
pub struct PlayerUnits<'w, 's> {
    avatars: Query<'w, 's, AvatarData, AvatarFilter>,
    living: Query<'w, 's, LivingData, LivingFilter>,
}

/// What a lookup of units by type reads of each: its stable id, its owner and its type.
pub(crate) type LivingData = (&'static StableId, &'static Owner, &'static UnitType);

/// The units a lookup by type visits: the living, the ones the client only receives too.
pub(crate) type LivingFilter = (Without<Dead>, Allow<Unpredicted>);

/// What a lookup of an avatar reads of each: its entity, its stable id and its owner.
pub(crate) type AvatarData = (Entity, &'static StableId, &'static Owner);

/// The units a lookup of an avatar visits: the avatars, the ones the client only receives too.
pub(crate) type AvatarFilter = (With<Avatar>, Allow<Unpredicted>);

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
        Commanded::avatar_among(self.avatars.iter(), slot)
    }

    /// Puts into `out`, cleared first, the living units `slot` owns of `unit_type`, by stable id,
    /// rising.
    pub fn typed(&self, slot: PlayerSlot, unit_type: UnitType, out: &mut Vec<StableId>) {
        out.clear();
        out.extend(
            self.living
                .iter()
                .filter(|&(_, owner, &typed)| owner.slot() == slot && typed == unit_type)
                .map(|(&id, ..)| id),
        );
        out.sort_unstable();
    }
}

impl Commanded {
    /// The avatar of `slot` among `avatars`: the lowest stable id should it own several.
    pub(crate) fn avatar_among<'a>(
        avatars: impl Iterator<Item = (Entity, &'a StableId, &'a Owner)>,
        slot: PlayerSlot,
    ) -> Option<Commanded> {
        avatars
            .filter(|&(_, _, owner)| owner.slot() == slot)
            .map(|(entity, &id, _)| Commanded { entity, id })
            .min_by_key(|unit| unit.id)
    }
}

impl HeldPlayerUnits {
    /// The param, of `world`, the world it was made of.
    pub fn get<'w, 's>(&'s mut self, world: &'w World) -> PlayerUnits<'w, 's> {
        self.0.get(world).expect("its params are always valid")
    }

    pub fn new(world: &mut World) -> HeldPlayerUnits {
        HeldPlayerUnits(SystemState::new(world))
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
