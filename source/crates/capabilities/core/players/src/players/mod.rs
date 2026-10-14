//! The players and the match as entities, under every capability: each player's entity and the
//! resources it holds, the match's entity, and where to find them.

pub(crate) mod capability_action;
pub(crate) mod held_out;
pub(crate) mod on_match;
pub(crate) mod order_units;
pub(crate) mod out_of_play;
pub(crate) mod player_resources;
pub(crate) mod player_rows;
pub(crate) mod players_call;
pub(crate) mod players_world;
pub(crate) mod resource_amount;
pub(crate) mod tick_commands;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declare::{Declarations, EntityParts, InstallCx, KindVisitor, Module};
use campfire_script::View;
use campfire_sim::{Breach, Capability, StableId};
use campfire_units::{MatchEntity, Player, UnitsWorld};
use campfire_values::{ResourceId, Team};

use crate::players::held_out::HeldOut;
use crate::players::out_of_play::OutOfPlay;
use crate::players::player_resources::PlayerResources;
use crate::players::players_call::PlayersCall;

/// The players and the match as entities: what spawns them, under every capability.
#[derive(Debug)]
pub struct Players;

impl Module for Players {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        Players::kinds(d);
        d.component::<MatchEntity>();
        d.component::<Player>();
        d.component::<HeldOut>();
        d.component::<OutOfPlay>();
        d.component::<PlayerResources>();
        d.call_part::<PlayersCall>();
        d.require::<Player, Team>();
        d.check::<MatchEntity>(Players::match_spawned);
        d.part::<Player, PlayerResources>(|at| {
            let resources = at.world().non_send::<View>().names().count::<ResourceId>();
            Some(PlayerResources::new(resources))
        });
    }

    fn install(_: &mut InstallCx<'_>) {}
}

impl Players {
    /// Visits the kinds of the players' and the match's entities, which state types name as
    /// their host.
    fn kinds<V: KindVisitor>(v: &mut V) {
        v.kind::<MatchEntity>();
        v.kind::<Player>();
    }

    /// Whether the match's entity stands, as every match that started spawns it, so no system
    /// finds it missing.
    fn match_spawned(world: &World) -> Result<(), Breach> {
        Breach::of_match(world.match_entity().is_some())
    }

    /// Spawns the match's entity, then each player's, in slot order, `teams` giving each slot's
    /// team, each with its named id and the parts the installed modules declare for its kind.
    pub fn spawn_match(world: &mut World, teams: impl Iterator<Item = Team>) {
        EntityParts::<MatchEntity>::give(&mut world.spawn((StableId::MATCH, MatchEntity)));
        for (slot, team) in (0..).map(PlayerSlot::new).zip(teams) {
            Players::spawn_player(world, slot, team);
        }
    }

    /// Spawns the entity of player `slot` on `team`, of its named id, with the parts the
    /// installed modules declare for a player, in a match whose entity spawned: no player is of a
    /// match that has none.
    fn spawn_player(world: &mut World, slot: PlayerSlot, team: Team) -> Entity {
        assert!(
            world.match_entity().is_some(),
            "players join a match that started, which holds its entity"
        );
        let mut player = world.spawn((StableId::of_player(slot), Player::new(slot), team));
        EntityParts::<Player>::give(&mut player);
        player.id()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_units::UnitsWorld;
    use campfire_values::{Relations, Team};

    use crate::players::Players;
    use crate::players::player_resources::PlayerResources;
    use crate::players::players_world::PlayersWorld;

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
            for slot in (0..players).map(PlayerSlot::new) {
                let player = Players::spawn_player(world, slot, Team::new(0));
                world
                    .entity_mut(player)
                    .insert(PlayerResources::new(resources));
            }
        }
    }
}
