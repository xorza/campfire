use bevy_ecs::query::{Has, QueryState};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::{CallError, CallPart, CallStart, Commit};

use crate::out_of_play::OutOfPlay;
use crate::player::Player;
use crate::player_resources::PlayerResources;
use crate::players_world::PlayersWorld;
use crate::seated::{Seated, Seating};

/// The players' part of a call: their resources as the call sees them, by slot, none in a match
/// with no mode, and whether the call changed them, which commit once its effects applied; whether
/// each slot was held as the match started; and whether each player is out of play. It reads them
/// as each call begins, through one query of the players, each in the place it held at the last
/// call, so a call allocates none.
#[derive(Debug, Default)]
pub struct PlayersCall {
    /// The players, their resources, whether each slot was held and whether each is out of play,
    /// made at the first call.
    players: Option<QueryState<PlayerParts>>,
    resources: Vec<PlayerResources>,
    seatings: Vec<Seating>,
    outs: Vec<bool>,
    written: bool,
}

/// The parts of a player the call reads.
type PlayerParts = (
    &'static Player,
    &'static PlayerResources,
    Has<Seated>,
    Has<OutOfPlay>,
);

impl PlayersCall {
    /// The resources of player `slot`, as the call changed them; `None` for a player the session
    /// lacks, or in a match with no mode.
    pub fn resources(&self, slot: PlayerSlot) -> Option<&PlayerResources> {
        self.resources.get(slot.index())
    }

    /// The resources of player `slot`, for the call to change; `None` for a player the session
    /// lacks, or in a match with no mode.
    pub fn resources_mut(&mut self, slot: PlayerSlot) -> Option<&mut PlayerResources> {
        self.written = true;
        self.resources.get_mut(slot.index())
    }

    /// Whether a player or a bot held `slot` as the match started; `None` for a player the
    /// session lacks, or in a match with no mode.
    pub fn seating(&self, slot: PlayerSlot) -> Option<Seating> {
        self.seatings.get(slot.index()).copied()
    }

    /// Whether player `slot` is out of play, as the last tick's end derived it; `None` for a
    /// player the session lacks, or in a match with no mode.
    pub fn is_out(&self, slot: PlayerSlot) -> Option<bool> {
        self.outs.get(slot.index()).copied()
    }
}

impl CallPart for PlayersCall {
    const COMMITS: Commit = Commit::AfterEffects;

    /// None: an earlier call's effects of the batch change what the players hold.
    fn begin_batch(&mut self, _: &World) {}

    fn begin(&mut self, world: &World, _: &CallStart) -> Result<(), CallError> {
        self.written = false;
        let players = self.players.get_or_insert_with(|| {
            QueryState::try_new(world).expect("the install registers every declared component")
        });
        players.update_archetypes(world);
        let (mut count, mut slots) = (0, 0);
        for (player, held, seated, out) in players.iter_manual(world) {
            let at = player.slot().index();
            if at >= self.resources.len() {
                self.resources
                    .resize_with(at + 1, || PlayerResources::new(0));
            }
            if at >= self.seatings.len() {
                self.seatings.resize(at + 1, Seating::Open);
                self.outs.resize(at + 1, false);
            }
            self.resources[at].clone_from(held);
            self.seatings[at] = if seated { Seating::Held } else { Seating::Open };
            self.outs[at] = out;
            count += 1;
            slots = slots.max(at + 1);
        }
        debug_assert_eq!(count, slots, "the players hold the slots from the first on");
        self.resources.truncate(count);
        self.seatings.truncate(count);
        self.outs.truncate(count);
        Ok(())
    }

    fn commit(&mut self, world: &mut World) {
        if !self.written {
            return;
        }
        for (slot, resources) in (0..).map(PlayerSlot::new).zip(&self.resources) {
            let mut held = world
                .player_resources_mut(slot)
                .expect("a player the call read");
            if *held != *resources {
                held.clone_from(resources);
            }
        }
    }
}
