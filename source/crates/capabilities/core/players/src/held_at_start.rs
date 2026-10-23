use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication};
use campfire_values::Team;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::player::Player;

/// Marks a player whose player slot a player or a bot held as the match started, on its entity,
/// which spawns with it: a player slot open then has none, so a mode places nothing for it, as Zero
/// Hour places no start for an empty player slot (09-zero-hour.md, Eight teams of one). A join or a
/// leave after the start changes it not; the mode hears those by their hooks.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub(crate) struct HeldAtStart;

/// Whether a player or a bot held a player slot as the match started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSlotHolder {
    Held,
    Open,
}

/// A player slot as the match starts: its team, and whether a player or a bot holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartingPlayerSlot {
    pub team: Team,
    pub holder: PlayerSlotHolder,
}

impl SimComponent for HeldAtStart {
    const NAME: &'static str = "players.held_at_start";
    type Host = Player;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for HeldAtStart {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
