use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::kinded::Kinded;
use crate::units::view::View;

/// The player who controls a unit, by slot: design 04's control relation, which `orders` and,
/// later, `character` read. A player may control many units.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Owner(PlayerSlot);

impl Owner {
    pub const fn new(slot: PlayerSlot) -> Owner {
        Owner(slot)
    }

    pub const fn slot(self) -> PlayerSlot {
        self.0
    }
}

impl SimComponent for Owner {
    const NAME: &'static str = "units.owner";

    // The script pools and the players' tables hold a place for each player only.
    fn check(&self, world: &World, _: Entity) -> bool {
        world
            .get_non_send::<View>()
            .is_none_or(|view| view.has_player(self.0))
    }
}

impl Kinded for Owner {
    const KIND: DataKind = DataKind::Unit;
}
