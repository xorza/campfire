use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::{EntityKind, KindAudience};
use campfire_sim::{KindMarker, SimComponent, StableId};
use campfire_states::{Replication, SentOnce};
use campfire_units::EntityKinds;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// The entity of the match, of the named id `StableId::MATCH`, which holds the match's state that
/// clients receive (10-replication-audiences.md, The match and each player are entities).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_add = EntityKinds::one)]
pub struct MatchEntity;

impl KindMarker for MatchEntity {}

impl EntityKind for MatchEntity {
    const AUDIENCE: KindAudience = KindAudience::EveryPlayerSlot;
    const OWNER_PREDICTS: bool = false;
}

impl SimComponent for MatchEntity {
    const NAME: &'static str = "players.match";
    type Host = Self;

    // An id not the match's would hide the match's state from where it is looked for; and an
    // entity is of one kind.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<StableId>(entity) == Some(&StableId::MATCH)
            && EntityKinds::is_alone(world, entity)
    }
}

impl Replication for MatchEntity {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
