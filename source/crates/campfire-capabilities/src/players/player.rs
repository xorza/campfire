use std::any::TypeId;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::{HostRule, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::declare::parted_kind::PartedKind;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::entity_kind::{EntityKind, KindAudience};
use crate::units::view::View;

/// The entity of a player slot, of the slot's named id, which holds the player's state that
/// clients receive (design 14, D7).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct Player(PlayerSlot);

impl Player {
    pub(crate) const fn new(slot: PlayerSlot) -> Player {
        Player(slot)
    }

    pub const fn slot(self) -> PlayerSlot {
        self.0
    }
}

impl PartedKind for Player {}

impl EntityKind for Player {
    const AUDIENCE: KindAudience = KindAudience::EverySeat;
    const OWNER_PREDICTS: bool = false;
    type Only = With<Player>;
}

impl HostRule for Player {
    fn holds(world: &World, entity: Entity) -> bool {
        world.get::<Player>(entity).is_some()
    }

    fn kinds(kinds: &mut Vec<TypeId>) -> bool {
        kinds.push(TypeId::of::<Player>());
        true
    }
}

impl SimComponent for Player {
    const NAME: &'static str = "players.player";
    type Host = Player;

    // A slot the session lacks has no place in the players' tables, and an id not the slot's
    // would hide the player's state from where it is looked for.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<StableId>(entity) == Some(&StableId::of_player(self.0))
            && world.non_send::<View>().has_player(self.0)
    }
}

impl Replication for Player {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
