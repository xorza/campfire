use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declarations::{Declarations, EntityParts, InstallCx, KindVisitor, Module, NoSystems};
use campfire_sim::{Breach, Capability, Either, StableId};
use campfire_units::{UnitEntity, UnitsWorld};
use campfire_values::{Relations, ResourceId, Team};

use crate::held_at_start::{HeldAtStart, PlayerSlotHolder, StartingPlayerSlot};
use crate::held_out::HeldOut;
use crate::match_entity::MatchEntity;
use crate::out_of_play::OutOfPlay;
use crate::player::Player;
use crate::player_resources::PlayerResources;
use crate::players_call::PlayersCall;
use crate::players_world::PlayersWorld;
use crate::resource_book::ResourceBook;
use crate::resources_data::ResourcesData;

/// The players and the match as entities: what spawns them, under every capability.
#[derive(Debug)]
pub struct Players;

impl Module for Players {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = NoSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ResourcesData>();
        d.book::<ResourceBook>(ResourceBook::build);
        d.names::<ResourceId>(ResourceBook::name_table);
        Self::kinds(d);
        d.component::<MatchEntity>();
        d.component::<Player>();
        d.component::<HeldOut>();
        d.component::<HeldAtStart>();
        d.component::<OutOfPlay>();
        d.component::<PlayerResources>();
        d.call_extension::<PlayersCall>();
        d.require::<Player, Team>();
        d.host::<Team, Either<UnitEntity, Player>>();
        d.host::<Relations, MatchEntity>();
        d.check::<MatchEntity>(Self::match_spawned);
        d.part::<Player, PlayerResources>(|at| {
            Some(PlayerResources::new(at.world().declared_resources()))
        });
    }

    fn install(_: &mut InstallCx<'_, Self::Systems>) {}
}

impl Players {
    /// Visits the kinds of the players' and the match's entities, which state types name as
    /// their host.
    fn kinds<V: KindVisitor>(v: &mut V) {
        v.kind::<MatchEntity>();
        v.kind::<Player>();
        v.state::<Player, OutOfPlay>();
    }

    /// Whether the match's entity stands, as every match that started spawns it, so no system
    /// finds it missing.
    fn match_spawned(world: &World) -> Result<(), Breach> {
        Breach::of_match(world.match_entity().is_some())
    }

    /// Spawns the match's entity, then each player's, in player slot order, `player_slots` giving
    /// each player slot's team and whether it is held, each with its named id and the parts the
    /// installed modules declare for its kind.
    pub fn spawn_match(
        world: &mut World,
        player_slots: impl IntoIterator<Item = StartingPlayerSlot>,
    ) {
        EntityParts::<MatchEntity>::spawn(world, (StableId::MATCH, MatchEntity));
        for (player_slot, start) in (0..).map(PlayerSlot::new).zip(player_slots) {
            Self::spawn_player(world, player_slot, start);
        }
    }

    /// Spawns the entity of player `player_slot`, on its team as `start` gives it, of its named id,
    /// with the parts the installed modules declare for a player, and marked `HeldAtStart` when its
    /// player slot is held, in one spawn, in a match whose entity spawned: no player is of a match
    /// that has none.
    fn spawn_player(
        world: &mut World,
        player_slot: PlayerSlot,
        start: StartingPlayerSlot,
    ) -> Entity {
        assert!(
            world.match_entity().is_some(),
            "players join a match that started, which holds its entity"
        );
        let core = (
            StableId::of_player(player_slot),
            Player::new(player_slot),
            start.team,
        );
        match start.holder {
            PlayerSlotHolder::Held => EntityParts::<Player>::spawn(world, (core, HeldAtStart)),
            PlayerSlotHolder::Open => EntityParts::<Player>::spawn(world, core),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_units::UnitsWorld;
    use campfire_values::{Relations, Team};

    use crate::held_at_start::{PlayerSlotHolder, StartingPlayerSlot};
    use crate::player_resources::PlayerResources;
    use crate::players::Players;
    use crate::players_world::PlayersWorld;

    impl Players {
        /// Starts the match, as a mode's start does: the match's entity spawns, of teams that
        /// regard each other as hostile, then the entity of each player of `teams`, in player_slot
        /// order, with none of each of `resources` resources.
        pub fn start_without_mode(
            world: &mut World,
            teams: impl IntoIterator<Item = Team>,
            resources: usize,
        ) {
            let teams: Vec<Team> = teams.into_iter().collect();
            let held = |team| StartingPlayerSlot {
                team,
                holder: PlayerSlotHolder::Held,
            };
            Self::spawn_match(world, teams.iter().copied().map(held));
            let entity = world.match_entity().expect("the match's entity spawned");
            world.entity_mut(entity).insert(Relations::default());
            for player_slot in (0..).map(PlayerSlot::new).take(teams.len()) {
                let player = world
                    .player_entity(player_slot)
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
            for player_slot in (0..players).map(PlayerSlot::new) {
                let open = StartingPlayerSlot {
                    team: Team::new(0),
                    holder: PlayerSlotHolder::Open,
                };
                let player = Self::spawn_player(world, player_slot, open);
                world
                    .entity_mut(player)
                    .insert(PlayerResources::new(resources));
            }
        }
    }
}
