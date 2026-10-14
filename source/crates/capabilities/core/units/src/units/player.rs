use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declare::{EntityKind, KindAudience};
use campfire_script::View;
use campfire_sim::{KindMarker, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentOnce};
use serde::{Deserialize, Serialize};

use crate::units::entity_kinds::EntityKinds;

/// The entity of a player slot, of the slot's named id, which holds the player's state that
/// clients receive (design 14, D7).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_add = EntityKinds::one)]
#[serde(transparent)]
pub struct Player(PlayerSlot);

impl Player {
    pub const fn new(slot: PlayerSlot) -> Player {
        Player(slot)
    }

    pub const fn slot(self) -> PlayerSlot {
        self.0
    }
}

impl KindMarker for Player {}

impl EntityKind for Player {
    const AUDIENCE: KindAudience = KindAudience::EverySeat;
    const OWNER_PREDICTS: bool = false;
}

impl SimComponent for Player {
    const NAME: &'static str = "players.player";
    type Host = Player;

    // A slot the session lacks has no place in the players' tables, and an id not the slot's
    // would hide the player's state from where it is looked for; and an entity is of one kind.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<StableId>(entity) == Some(&StableId::of_player(self.0))
            && world.non_send::<View>().names().has_player(self.0)
            && EntityKinds::alone(world, entity)
    }
}

impl Replication for Player {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
