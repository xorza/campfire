use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declare::{Declarations, EntityParts, InstallCx, KindVisitor, Module, NoSet};
use campfire_sim::{Breach, Capability, StableId};
use campfire_units::{MatchEntity, Player, UnitsWorld};
use campfire_values::Team;

use crate::held_out::HeldOut;
use crate::out_of_play::OutOfPlay;
use crate::player_resources::PlayerResources;
use crate::players_call::PlayersCall;
use crate::players_world::PlayersWorld;
use crate::resource_book::ResourceBook;
use crate::resources_data::ResourcesData;
use crate::seated::{Seated, Seating, SlotStart};

/// The players and the match as entities: what spawns them, under every capability.
#[derive(Debug)]
pub struct Players;

impl Module for Players {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Set = NoSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ResourcesData>();
        d.book::<ResourceBook>(ResourceBook::build);
        Self::kinds(d);
        d.component::<MatchEntity>();
        d.component::<Player>();
        d.component::<HeldOut>();
        d.component::<Seated>();
        d.component::<OutOfPlay>();
        d.component::<PlayerResources>();
        d.call_part::<PlayersCall>();
        d.require::<Player, Team>();
        d.check::<MatchEntity>(Self::match_spawned);
        d.part::<Player, PlayerResources>(|at| {
            Some(PlayerResources::new(at.world().declared_resources()))
        });
    }

    fn install(_: &mut InstallCx<'_, Self::Set>) {}
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

    /// Spawns the match's entity, then each player's, in slot order, `slots` giving each slot's
    /// team and whether it is held, each with its named id and the parts the installed modules
    /// declare for its kind.
    pub fn spawn_match(world: &mut World, slots: impl IntoIterator<Item = SlotStart>) {
        EntityParts::<MatchEntity>::give(&mut world.spawn((StableId::MATCH, MatchEntity)));
        for (slot, start) in (0..).map(PlayerSlot::new).zip(slots) {
            Self::spawn_player(world, slot, start);
        }
    }

    /// Spawns the entity of player `slot`, on its team as `start` gives it, of its named id, with
    /// the parts the installed modules declare for a player, and marked `Seated` when its slot is
    /// held, in one spawn, in a match whose entity spawned: no player is of a match that has
    /// none.
    fn spawn_player(world: &mut World, slot: PlayerSlot, start: SlotStart) -> Entity {
        assert!(
            world.match_entity().is_some(),
            "players join a match that started, which holds its entity"
        );
        let core = (StableId::of_player(slot), Player::new(slot), start.team);
        let mut player = match start.seating {
            Seating::Held => world.spawn((core, Seated)),
            Seating::Open => world.spawn(core),
        };
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

    use crate::player_resources::PlayerResources;
    use crate::players::Players;
    use crate::players_world::PlayersWorld;
    use crate::seated::{Seating, SlotStart};

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
            let held = |team| SlotStart {
                team,
                seating: Seating::Held,
            };
            Self::spawn_match(world, teams.iter().copied().map(held));
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
                let open = SlotStart {
                    team: Team::new(0),
                    seating: Seating::Open,
                };
                let player = Self::spawn_player(world, slot, open);
                world
                    .entity_mut(player)
                    .insert(PlayerResources::new(resources));
            }
        }
    }
}
