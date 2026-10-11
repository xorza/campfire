use std::any::TypeId;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_sim::{EntityIndex, HostRule, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::declare::parted_kind::PartedKind;
use crate::players::kind_parts::KindParts;
use crate::players::player::Player;
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
    /// Spawns the match's entity, then each player's, in slot order, `teams` giving each slot's
    /// team, each with its named id and the parts the installed modules declare for its kind.
    pub(crate) fn spawn(world: &mut World, teams: impl Iterator<Item = Team>) {
        world.resource_scope(|world, parts: Mut<'_, KindParts<MatchEntity>>| {
            parts.put_on(&mut world.spawn((StableId::MATCH, MatchEntity)));
        });
        world.resource_scope(|world, parts: Mut<'_, KindParts<Player>>| {
            for (slot, team) in (0..).map(PlayerSlot::new).zip(teams) {
                MatchEntity::spawn_player_of(world, &parts, slot, team);
            }
        });
    }

    /// Spawns the entity of player `slot` on `team`, of its named id, with `parts`.
    fn spawn_player_of(
        world: &mut World,
        parts: &KindParts<Player>,
        slot: PlayerSlot,
        team: Team,
    ) -> Entity {
        let mut player = world.spawn((StableId::of_player(slot), Player::new(slot), team));
        parts.put_on(&mut player);
        player.id()
    }

    /// The match's entity in `world`, once it spawned.
    pub fn entity(world: &World) -> Option<Entity> {
        world.resource::<EntityIndex>().get(StableId::MATCH)
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

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::iter;

    use bevy_ecs::world::World;

    use crate::players::match_entity::MatchEntity;
    use crate::units::relations::Relations;

    impl MatchEntity {
        /// Starts a match in `world` that has no mode to start it: the match's entity spawns, of
        /// teams that regard each other as hostile, with no player.
        pub fn start_without_mode(world: &mut World) {
            MatchEntity::spawn(world, iter::empty());
            let entity = MatchEntity::entity(world).expect("the match's entity spawned");
            world.entity_mut(entity).insert(Relations::default());
        }
    }
}

#[cfg(any(test, feature = "bench"))]
pub(crate) mod test_players {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::{Mut, World};
    use campfire_common::PlayerSlot;

    use crate::players::kind_parts::KindParts;
    use crate::players::match_entity::MatchEntity;
    use crate::players::player::Player;
    use crate::players::player_resources::PlayerResources;
    use crate::units::team::Team;

    impl MatchEntity {
        /// Spawns, in a match whose mode does not start it, the entity of player `slot` on `team`,
        /// with what the installed capabilities put on it and none of each of `resources`
        /// resources, as a mode's start does.
        pub(crate) fn spawn_player(
            world: &mut World,
            slot: PlayerSlot,
            team: Team,
            resources: usize,
        ) -> Entity {
            world.resource_scope(|world, parts: Mut<'_, KindParts<Player>>| {
                let player = MatchEntity::spawn_player_of(world, &parts, slot, team);
                world
                    .entity_mut(player)
                    .insert(PlayerResources::new(resources));
                player
            })
        }

        /// Spawns the entities of players 0 to `players`, all on team 0, as `spawn_player` does.
        #[cfg(test)]
        pub(crate) fn spawn_players(world: &mut World, players: u32, resources: usize) {
            for slot in (0..players).map(PlayerSlot::new) {
                MatchEntity::spawn_player(world, slot, Team::new(0), resources);
            }
        }
    }
}
