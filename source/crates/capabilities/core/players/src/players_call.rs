use bevy_ecs::query::{Has, QueryState};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::{CallError, CallPart, CallStart, Commit};
use campfire_units::Player;

use crate::player_resources::PlayerResources;
use crate::players_world::PlayersWorld;
use crate::seated::{Seated, Seating};

/// The players' part of a call: their resources as the call sees them, by slot, none in a match
/// with no mode, and whether the call changed them, which commit once its effects applied; and
/// whether each slot was held as the match started. It reads them as each call begins, through
/// one query of the players, each in the place it held at the last call, so a call allocates
/// none.
#[derive(Debug, Default)]
pub struct PlayersCall {
    /// The players, their resources, and whether each slot was held, made at the first call.
    players: Option<QueryState<(&'static Player, &'static PlayerResources, Has<Seated>)>>,
    resources: Vec<PlayerResources>,
    seatings: Vec<Seating>,
    written: bool,
}

impl PlayersCall {
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
}

impl CallPart for PlayersCall {
    const COMMITS: Commit = Commit::AfterEffects;

    fn begin(&mut self, world: &World, _: &CallStart) -> Result<(), CallError> {
        self.written = false;
        let players = self.players.get_or_insert_with(|| {
            QueryState::try_new(world).expect("the install registers every declared component")
        });
        players.update_archetypes(world);
        let (mut count, mut slots) = (0, 0);
        for (player, held, seated) in players.iter_manual(world) {
            let at = player.slot().index();
            if at >= self.resources.len() {
                self.resources
                    .resize_with(at + 1, || PlayerResources::new(0));
            }
            if at >= self.seatings.len() {
                self.seatings.resize(at + 1, Seating::Open);
            }
            self.resources[at].clone_from(held);
            self.seatings[at] = if seated { Seating::Held } else { Seating::Open };
            count += 1;
            slots = slots.max(at + 1);
        }
        debug_assert_eq!(count, slots, "the players hold the slots from the first on");
        self.resources.truncate(count);
        self.seatings.truncate(count);
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
