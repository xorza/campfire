use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_values::Team;
use serde::{Deserialize, Serialize};

use crate::player::Player;

/// Marks a player whose slot a player or a bot held as the match started, on its entity, which
/// spawns with it: a slot open then has none, so a mode places nothing for it, as Zero Hour places
/// no start for an empty slot (12-zero-hour.md, Eight teams of one). A join or a leave after the
/// start changes it not; the mode hears those by their hooks.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct Seated;

/// Whether a player or a bot held a slot as the match started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seating {
    Held,
    Open,
}

/// A slot as the match starts: its team, and whether a player or a bot holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotStart {
    pub team: Team,
    pub seating: Seating,
}

impl SimComponent for Seated {
    const NAME: &'static str = "players.seated";
    type Host = Player;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Seated {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
