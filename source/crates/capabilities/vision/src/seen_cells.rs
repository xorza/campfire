use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::cell_bits::CellBits;

/// The cells a player's team saw in the last Vision stage, on its entity
/// (04-capabilities/vision.md, The server sends a team's cells to its player): the server derives
/// it as the stage ends and sends it to its player on each change; no hash or snapshot holds it.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenCells(pub(crate) CellBits);

impl SeenCells {
    pub const fn cells(&self) -> &CellBits {
        &self.0
    }
}

impl SimComponent for SeenCells {
    const NAME: &'static str = "vision.seen_cells";
    type Host = Player;

    // Derived, so no snapshot holds one for a restore to check.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for SeenCells {
    const KIND: DataKind = DataKind::Sight;
    type Sending = SentOnChange;
}
