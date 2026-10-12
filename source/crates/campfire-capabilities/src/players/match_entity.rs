use std::any::TypeId;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::{HostRule, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::declare::parted_kind::PartedKind;
use crate::players::kind_parts::KindParts;
use crate::players::player::Player;
use crate::players::players_world::PlayersWorld;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::entity_kind::{EntityKind, KindAudience};
use crate::units::team::Team;

/// The entity of the match, of the named id `StableId::MATCH`, which holds the match's state that
/// clients receive (design 14, D7).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct MatchEntity;

impl MatchEntity {
    /// Spawns the entity of player `slot` on `team`, of its named id, with `parts`, in a match
    /// whose entity spawned: no player is of a match that has none.
    pub(super) fn spawn_player_of(
        world: &mut World,
        parts: &KindParts<Player>,
        slot: PlayerSlot,
        team: Team,
    ) -> Entity {
        assert!(
            world.match_entity().is_some(),
            "players join a match that started, which holds its entity"
        );
        let mut player = world.spawn((StableId::of_player(slot), Player::new(slot), team));
        parts.put_on(&mut player);
        player.id()
    }
}

impl PartedKind for MatchEntity {}

impl EntityKind for MatchEntity {
    const AUDIENCE: KindAudience = KindAudience::EverySeat;
    const OWNER_PREDICTS: bool = false;
    type Only = With<MatchEntity>;
}

impl HostRule for MatchEntity {
    fn holds(world: &World, entity: Entity) -> bool {
        world.get::<MatchEntity>(entity).is_some()
    }

    fn kinds(kinds: &mut Vec<TypeId>) -> bool {
        kinds.push(TypeId::of::<MatchEntity>());
        true
    }
}

impl SimComponent for MatchEntity {
    const NAME: &'static str = "players.match";
    type Host = MatchEntity;

    // An id not the match's would hide the match's state from where it is looked for.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<StableId>(entity) == Some(&StableId::MATCH)
    }
}

impl Replication for MatchEntity {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
