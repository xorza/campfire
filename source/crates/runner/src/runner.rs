use bevy_ecs::world::World;
use campfire_protocol::{
    Applied, InputError, PlayerInput, SeedError, ServerSeed, SessionHeader, SessionLog,
};
use campfire_sim::StateHash;

use crate::session::Session;

/// A match in a bare `World`, with no network layer: what a verifier replays a log in.
#[derive(Debug)]
pub struct Runner {
    world: World,
}

impl Runner {
    /// See `Session::start`.
    pub fn new(header: SessionHeader, server_seed: ServerSeed) -> Result<Runner, SeedError> {
        let mut world = World::new();
        Session::start(&mut world, header, server_seed)?;
        Ok(Runner { world })
    }

    pub fn record(&mut self, input: PlayerInput<'_>) -> Result<Applied, InputError> {
        self.world.resource_mut::<Session>().record(input)
    }

    pub fn run_tick(&mut self) {
        Session::run_tick(&mut self.world);
    }

    pub fn reveal_seed(&mut self) {
        self.world.resource_mut::<Session>().reveal_seed();
    }

    pub fn state_hash(&self) -> StateHash {
        self.world.resource::<Session>().state_hash(&self.world)
    }

    pub fn log(&self) -> &SessionLog {
        self.world.resource::<Session>().log()
    }

    pub const fn world(&self) -> &World {
        &self.world
    }
}
