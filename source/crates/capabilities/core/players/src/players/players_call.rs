use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::{CallError, CallPart, CallStart, ScriptRole, ScriptValue};

use crate::players::player_resources::PlayerResources;
use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};

/// The players' part of a call: their resources as the call sees them, by slot, none in a match
/// with no mode, and whether the call changed them, which apply once its effects did. It reads
/// them as each call begins, each in the place it held at the last call, so a call allocates
/// none.
#[derive(Debug, Default)]
pub struct PlayersCall {
    resources: Vec<PlayerResources>,
    written: bool,
}

impl PlayersCall {
    /// The resources of player `slot`, for the call to change; `None` for a player the session
    /// lacks, or in a match with no mode.
    pub fn resources_mut(&mut self, slot: PlayerSlot) -> Option<&mut PlayerResources> {
        self.written = true;
        self.resources.get_mut(slot.index())
    }
}

impl CallPart for PlayersCall {
    fn begin(&mut self, world: &World, _: &CallStart) -> Result<(), CallError> {
        self.written = false;
        let held = (0..).map_while(|slot| {
            let entity = world.player_entity(PlayerSlot::new(slot))?;
            world.get::<PlayerResources>(entity)
        });
        let mut players = 0;
        for (at, held) in held.enumerate() {
            match self.resources.get_mut(at) {
                Some(resources) => resources.clone_from(held),
                None => self.resources.push(held.clone()),
            }
            players = at + 1;
        }
        self.resources.truncate(players);
        Ok(())
    }

    fn param(&self, _: Option<ScriptRole>, _: &str) -> Option<ScriptValue> {
        None
    }

    fn apply(&mut self, world: &mut World) {
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
