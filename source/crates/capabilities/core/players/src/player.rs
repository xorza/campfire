use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Seat;
use campfire_declare::{EntityKind, KindAudience};
use campfire_script::MatchNames;
use campfire_sim::{KindMarker, SimComponent, StableId};
use campfire_state_types::{Replication, SentOnce};
use campfire_units::EntityKinds;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// The entity of a player seat, of the seat's named id, which holds the player's state that
/// clients receive (10-replication-audiences.md, The match and each player are entities).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_add = EntityKinds::one)]
#[serde(transparent)]
pub struct Player(Seat);

impl Player {
    pub const fn new(seat: Seat) -> Self {
        Self(seat)
    }

    pub const fn seat(self) -> Seat {
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
    type Host = Self;

    // A seat the session lacks has no place in the players' tables, and an id not the seat's
    // would hide the player's state from where it is looked for; and an entity is of one kind.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<StableId>(entity) == Some(&StableId::of_player(self.0))
            && world.resource::<MatchNames>().has_player(self.0)
            && EntityKinds::is_alone(world, entity)
    }
}

impl Replication for Player {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
