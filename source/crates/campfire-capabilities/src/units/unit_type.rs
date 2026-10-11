use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::view::View;

/// A unit's type, by its place in the match's unit types.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct UnitType(u16);

impl UnitType {
    pub(crate) const fn new(index: u16) -> UnitType {
        UnitType(index)
    }

    /// Its place in the match's unit types, from 0.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for UnitType {
    const NAME: &'static str = "units.unit_type";

    // A type the match did not load names nothing its books hold, and a unit of a named id
    // would stand where a player's or the match's entity is looked for.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<StableId>(entity)
            .is_some_and(|id| id.is_allocated())
            && world
                .get_non_send::<View>()
                .is_none_or(|view| view.has_type(*self))
    }
}

impl Replication for UnitType {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
