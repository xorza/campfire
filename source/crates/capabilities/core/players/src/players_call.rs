use bevy_ecs::query::{Has, QueryState};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::{CallError, CallExtension, CallStart, Commit};

use crate::held_at_start::{HeldAtStart, PlayerSlotHolder};
use crate::out_of_play::OutOfPlay;
use crate::player::Player;
use crate::player_resources::PlayerResources;
use crate::players_world::PlayersWorld;

/// The players' part of a call: their resources as the call sees them, by player slot, none in a
/// match with no mode, and whether the call changed them, which commit before its effects apply, so
/// an effect that pays from them pays from what the call left; whether each player slot was held as
/// the match started; and whether each player is out of play. It reads the resources as each call
/// begins, and the holders and the outs, which no call changes, as each batch begins, through one
/// query of the players, each in the place it held at the last read, so a call allocates none.
#[derive(Debug, Default)]
pub struct PlayersCall {
    /// The players, their resources, whether each player slot was held and whether each is out of
    /// play, made at the first call.
    players: Option<QueryState<PlayerParts>>,
    resources: Vec<PlayerResources>,
    holders: Vec<PlayerSlotHolder>,
    outs: Vec<bool>,
    written: bool,
}

/// The parts of a player the call reads.
type PlayerParts = (
    &'static Player,
    &'static PlayerResources,
    Has<HeldAtStart>,
    Has<OutOfPlay>,
);

impl PlayersCall {
    /// The resources of player `player_slot`, as the call changed them; `None` for a player the
    /// session lacks, or in a match with no mode.
    pub fn resources(&self, player_slot: PlayerSlot) -> Option<&PlayerResources> {
        self.resources.get(player_slot.index())
    }

    /// The resources of player `player_slot`, for the call to change; `None` for a player the
    /// session lacks, or in a match with no mode.
    pub fn resources_mut(&mut self, player_slot: PlayerSlot) -> Option<&mut PlayerResources> {
        self.written = true;
        self.resources.get_mut(player_slot.index())
    }

    /// Whether a player or a bot held `player_slot` as the match started; `None` for a player the
    /// session lacks, or in a match with no mode.
    pub fn holder(&self, player_slot: PlayerSlot) -> Option<PlayerSlotHolder> {
        self.holders.get(player_slot.index()).copied()
    }

    /// Whether player `player_slot` is out of play, as the last tick's end derived it; `None` for a
    /// player the session lacks, or in a match with no mode.
    pub fn is_out(&self, player_slot: PlayerSlot) -> Option<bool> {
        self.outs.get(player_slot.index()).copied()
    }
}

impl PlayersCall {
    /// The players' query, made at the first read, its archetypes as `world` holds them.
    fn players(&mut self, world: &World) -> &QueryState<PlayerParts> {
        let players = self.players.get_or_insert_with(|| {
            QueryState::try_new(world).expect("the install registers every declared component")
        });
        players.update_archetypes(world);
        players
    }
}

impl CallExtension for PlayersCall {
    const COMMITS: Commit = Commit::BeforeEffects;

    /// Reads the holders and the outs, which no call of the batch changes.
    fn begin_batch(&mut self, world: &World) {
        let (mut holders, mut outs) = (
            std::mem::take(&mut self.holders),
            std::mem::take(&mut self.outs),
        );
        holders.clear();
        outs.clear();
        for (player, _, held, out) in self.players(world).iter_manual(world) {
            let at = player.player_slot().index();
            if at >= holders.len() {
                holders.resize(at + 1, PlayerSlotHolder::Open);
                outs.resize(at + 1, false);
            }
            holders[at] = if held {
                PlayerSlotHolder::Held
            } else {
                PlayerSlotHolder::Open
            };
            outs[at] = out;
        }
        self.holders = holders;
        self.outs = outs;
    }

    /// Reads the resources, which an earlier call's effects of the batch change.
    fn begin(&mut self, world: &World, _: &CallStart) -> Result<(), CallError> {
        self.written = false;
        let mut resources = std::mem::take(&mut self.resources);
        let (mut count, mut player_slots) = (0, 0);
        for (player, held, ..) in self.players(world).iter_manual(world) {
            let at = player.player_slot().index();
            if at >= resources.len() {
                resources.resize_with(at + 1, || PlayerResources::new(0));
            }
            resources[at].clone_from(held);
            count += 1;
            player_slots = player_slots.max(at + 1);
        }
        debug_assert_eq!(
            count, player_slots,
            "the players hold the player slots from the first on"
        );
        resources.truncate(count);
        self.resources = resources;
        Ok(())
    }

    fn commit(&mut self, world: &mut World) {
        if !self.written {
            return;
        }
        for (player_slot, resources) in (0..).map(PlayerSlot::new).zip(&self.resources) {
            let mut held = world
                .player_resources_mut(player_slot)
                .expect("a player the call read");
            if *held != *resources {
                held.clone_from(resources);
            }
        }
    }
}
