use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::View;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnInsert};
use campfire_values::Team;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// The player who controls a unit, by slot: the control relation (04-capabilities/00-overview.md,
/// Control is a relation to units), which `orders` and, later, `character` read. A player may
/// control many units, each of the player's team. Immutable, so a change of owner is a new one
/// inserted, which every data filter follows (14-replication-audiences.md, An owner changes by an
/// insert alone; 14-replication-audiences.md, A unit's owner is a player of its team).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct Owner(PlayerSlot);

impl Owner {
    pub const fn new(slot: PlayerSlot) -> Self {
        Self(slot)
    }

    pub const fn slot(self) -> PlayerSlot {
        self.0
    }
}

impl SimComponent for Owner {
    const NAME: &'static str = "units.owner";
    type Host = UnitEntity;

    // The script pools and the players' tables hold a place for each player only, and a unit's
    // owner is of its team, whose vision its client holds the unit by.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<Team>(entity)
            .is_some_and(|&team| world.non_send::<View>().names().plays_for(self.0, team))
    }
}

impl Replication for Owner {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}
