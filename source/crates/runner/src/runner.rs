use bevy_ecs::world::World;
use campfire_protocol::{Applied, ChainSignature, InputError, PlayerInput, ServerSeed, SessionLog};
use campfire_sim::StateHash;

use crate::error::StartError;
use crate::mode_packages::ModePackages;
use crate::session::Session;

/// A match in a bare `World`, with no network layer: what a verifier replays a log in.
#[derive(Debug)]
pub struct Runner {
    world: World,
}

impl Runner {
    /// See `Session::start`.
    pub fn new(
        log: SessionLog,
        server_seed: ServerSeed,
        packages: &ModePackages,
    ) -> Result<Runner, StartError> {
        let mut world = World::new();
        Session::start(&mut world, log, server_seed, packages)?;
        Ok(Runner { world })
    }

    /// See `Session::record`.
    pub fn record<'a, I>(
        &mut self,
        inputs: I,
        signature: &ChainSignature,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = PlayerInput<'a>>,
        I::IntoIter: Clone,
    {
        self.world
            .resource_mut::<Session>()
            .record(inputs, signature, applied)
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
