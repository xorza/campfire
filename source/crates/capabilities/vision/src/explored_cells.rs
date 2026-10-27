use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::cell_bits::CellBits;
use crate::vision_book::VisionBook;

/// The cells a player's team ever saw, on its entity, of a map that starts shrouded, none of one
/// that starts known (04-capabilities/vision.md, A team keeps the cells it explored): state, as no
/// stage derives them again, which the server sends to its player on each change.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExploredCells(pub(crate) CellBits);

impl ExploredCells {
    pub const fn cells(&self) -> &CellBits {
        &self.0
    }

    /// The cells a player of a match of `vision` starts with: none explored of a shrouded map's
    /// grid, and no word of another.
    pub(crate) fn start(vision: &VisionBook) -> Self {
        let grid = vision.grid().filter(|grid| grid.shroud);
        Self(grid.map_or_else(CellBits::default, |grid| CellBits::empty(grid.grid.cells())))
    }
}

impl SimComponent for ExploredCells {
    const NAME: &'static str = "vision.explored_cells";
    type Host = Player;

    // Words of another count would place a cell elsewhere on the grid.
    fn check(&self, world: &World, _: Entity) -> bool {
        Self::start(world.resource::<VisionBook>()).0.words().len() == self.0.words().len()
    }
}

impl Replication for ExploredCells {
    const KIND: DataKind = DataKind::Sight;
    type Sending = SentOnChange;
}
