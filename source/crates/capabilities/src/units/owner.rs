use bevy_ecs::component::Component;
use campfire_sim::{PlayerSlot, SimComponent};
use serde::{Deserialize, Serialize};

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
}
