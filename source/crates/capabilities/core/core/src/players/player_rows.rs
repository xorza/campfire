use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::query::Allow;
use bevy_ecs::system::{Query, Res, SystemParam};
use bevy_ecs::world::Mut;
use campfire_common::PlayerSlot;
use campfire_sim::{EntityIndex, StableId, Unpredicted};

use crate::units::owner::Owner;

/// Each player's `C`, on its entity, as a system reads them: a client holds the entities as the
/// server sends them, unpredicted.
#[derive(SystemParam, Debug)]
pub struct PlayerRows<'w, 's, C: Component> {
    index: Res<'w, EntityIndex>,
    rows: Query<'w, 's, &'static C, Allow<Unpredicted>>,
}

impl<C: Component> PlayerRows<'_, '_, C> {
    /// The `C` of the player that owns a unit of `owner`: none for a unit no player owns, or in a
    /// match whose players hold none.
    pub fn of(&self, owner: Option<&Owner>) -> Option<&C> {
        self.of_slot(owner?.slot())
    }

    /// The `C` of player `slot`.
    pub fn of_slot(&self, slot: PlayerSlot) -> Option<&C> {
        let entity = self.index.get(StableId::of_player(slot))?;
        self.rows.get(entity).ok()
    }
}

/// Each player's `C`, on its entity, as a system changes them.
#[derive(SystemParam, Debug)]
pub struct PlayerRowsMut<'w, 's, C: Component<Mutability = Mutable>> {
    index: Res<'w, EntityIndex>,
    rows: Query<'w, 's, &'static mut C, Allow<Unpredicted>>,
}

impl<C: Component<Mutability = Mutable>> PlayerRowsMut<'_, '_, C> {
    /// The `C` of the player that owns a unit of `owner`, as `PlayerRows::of` finds it.
    pub fn of(&self, owner: Option<&Owner>) -> Option<&C> {
        let entity = self.index.get(StableId::of_player(owner?.slot()))?;
        self.rows.get(entity).ok()
    }

    /// The `C` of the player that owns a unit of `owner`, to change.
    pub fn of_mut(&mut self, owner: Option<&Owner>) -> Option<Mut<'_, C>> {
        self.of_slot_mut(owner?.slot())
    }

    /// The `C` of player `slot`, to change.
    pub fn of_slot_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, C>> {
        let entity = self.index.get(StableId::of_player(slot))?;
        self.rows.get_mut(entity).ok()
    }
}
