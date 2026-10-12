//! The players and the match as entities, under every capability: each player's entity and the
//! resources it holds, the match's entity, and where to find them.

pub(crate) mod held_out;
pub(crate) mod kind_parts;
pub(crate) mod match_entity;
pub(crate) mod on_match;
pub(crate) mod out_of_play;
pub(crate) mod player;
pub(crate) mod player_resources;
pub(crate) mod player_rows;
pub(crate) mod players_world;
pub(crate) mod resource_amount;
pub(crate) mod resource_id;

use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_sim::StableId;

use crate::players::kind_parts::KindParts;
use crate::players::match_entity::MatchEntity;
use crate::players::player::Player;
use crate::units::team::Team;

/// The players and the match as entities: what spawns them, under every capability.
#[derive(Debug)]
#[cfg_attr(
    not(feature = "internals"),
    expect(
        unreachable_pub,
        reason = "the internals facade exports it, for its spawns"
    )
)]
pub struct Players;

impl Players {
    /// Spawns the match's entity, then each player's, in slot order, `teams` giving each slot's
    /// team, each with its named id and the parts the installed modules declare for its kind.
    pub(crate) fn spawn_match(world: &mut World, teams: impl Iterator<Item = Team>) {
        world.resource_scope(|world, parts: Mut<'_, KindParts<MatchEntity>>| {
            parts.put_on(&mut world.spawn((StableId::MATCH, MatchEntity)));
        });
        world.resource_scope(|world, parts: Mut<'_, KindParts<Player>>| {
            for (slot, team) in (0..).map(PlayerSlot::new).zip(teams) {
                MatchEntity::spawn_player_of(world, &parts, slot, team);
            }
        });
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::{Mut, World};
    use campfire_common::PlayerSlot;

    use crate::players::Players;
    use crate::players::kind_parts::KindParts;
    use crate::players::match_entity::MatchEntity;
    use crate::players::player::Player;
    use crate::players::player_resources::PlayerResources;
    use crate::players::players_world::PlayersWorld;
    use crate::units::relations::Relations;
    use crate::units::team::Team;

    impl Players {
        /// Starts the match, as a mode's start does: the match's entity spawns, of teams that
        /// regard each other as hostile, then the entity of each player of `teams`, in slot
        /// order, with none of each of `resources` resources.
        pub fn start_without_mode(
            world: &mut World,
            teams: impl IntoIterator<Item = Team>,
            resources: usize,
        ) {
            let teams: Vec<Team> = teams.into_iter().collect();
            Players::spawn_match(world, teams.iter().copied());
            let entity = world.match_entity().expect("the match's entity spawned");
            world.entity_mut(entity).insert(Relations::default());
            for slot in (0..).map(PlayerSlot::new).take(teams.len()) {
                let player = world
                    .player_entity(slot)
                    .expect("each player's entity spawned");
                world
                    .entity_mut(player)
                    .insert(PlayerResources::new(resources));
            }
        }

        /// Spawns, in a match that started, the entities of players 0 to `players`, all on team
        /// 0, with what the installed capabilities put on them and none of each of `resources`
        /// resources, as a mode's start does.
        pub fn spawn(world: &mut World, players: u32, resources: usize) {
            world.resource_scope(|world, parts: Mut<'_, KindParts<Player>>| {
                for slot in (0..players).map(PlayerSlot::new) {
                    let player = MatchEntity::spawn_player_of(world, &parts, slot, Team::new(0));
                    world
                        .entity_mut(player)
                        .insert(PlayerResources::new(resources));
                }
            });
        }
    }
}
