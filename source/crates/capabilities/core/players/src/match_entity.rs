use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::{EntityKind, KindAudience};
use campfire_sim::{KindMarker, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentOnce};
use campfire_units::EntityKinds;
use serde::{Deserialize, Serialize};

/// The entity of the match, of the named id `StableId::MATCH`, which holds the match's state that
/// clients receive (14-replication-audiences.md, The match and each player are entities).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_add = EntityKinds::one)]
pub struct MatchEntity;

impl KindMarker for MatchEntity {}

impl EntityKind for MatchEntity {
    const AUDIENCE: KindAudience = KindAudience::EverySeat;
    const OWNER_PREDICTS: bool = false;
}

impl SimComponent for MatchEntity {
    const NAME: &'static str = "players.match";
    type Host = Self;

    // An id not the match's would hide the match's state from where it is looked for; and an
    // entity is of one kind.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<StableId>(entity) == Some(&StableId::MATCH) && EntityKinds::alone(world, entity)
    }
}

impl Replication for MatchEntity {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
