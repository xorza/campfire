//! The players and the match as entities, under every capability: each player's entity and the
//! resources it holds, the match's entity, and where to find them.

pub(crate) mod held_out;
pub(crate) mod match_entity;
pub(crate) mod on_match;
pub(crate) mod out_of_play;
pub(crate) mod player;
pub(crate) mod player_resources;
pub(crate) mod player_rows;
pub(crate) mod players_world;
pub(crate) mod resource_amount;
pub(crate) mod resource_id;

use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::{Breach, Capability, StableId};

use crate::declare::declarations::Declarations;
use crate::declare::entity_parts::EntityParts;
use crate::declare::install_cx::InstallCx;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::module::Module;
use crate::players::held_out::HeldOut;
use crate::players::match_entity::MatchEntity;
use crate::players::out_of_play::OutOfPlay;
use crate::players::player::Player;
use crate::players::player_resources::PlayerResources;
use crate::players::players_world::PlayersWorld;
use crate::units::team::Team;
use crate::units::view::View;

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

impl Module for Players {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        Players::kinds(d);
        d.component::<MatchEntity>();
        d.component::<Player>();
        d.component::<HeldOut>();
        d.component::<OutOfPlay>();
        d.component::<PlayerResources>();
        d.require::<Player, Team>();
        d.check::<MatchEntity>(Players::match_spawned);
        d.part::<Player, PlayerResources>(|at| {
            let resources = at.world().non_send::<View>().names().resources.len();
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
    pub(crate) fn spawn_match(world: &mut World, teams: impl Iterator<Item = Team>) {
        EntityParts::<MatchEntity>::give(&mut world.spawn((StableId::MATCH, MatchEntity)));
        for (slot, team) in (0..).map(PlayerSlot::new).zip(teams) {
            MatchEntity::spawn_player_of(world, slot, team);
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;

    use crate::players::Players;
    use crate::players::match_entity::MatchEntity;
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
            for slot in (0..players).map(PlayerSlot::new) {
                let player = MatchEntity::spawn_player_of(world, slot, Team::new(0));
                world
                    .entity_mut(player)
                    .insert(PlayerResources::new(resources));
            }
        }
    }
}
