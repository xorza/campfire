use bevy_ecs::world::World;
use campfire_common::StateHash;
use campfire_package::ModePackages;
use campfire_protocol::{
    Applied, Checkpoint, CheckpointError, InputError, Journal, Outcome, PlayerInput, ResultError,
    ServerInput, ServerSeeds, SessionLog, SessionResult, Signature,
};

use crate::error::{ResultMismatch, ServerInputRefused, StartError};
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
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<Runner, StartError> {
        let mut world = World::new();
        Session::start(&mut world, log, seeds, packages)?;
        Ok(Runner { world })
    }

    /// See `Session::record`.
    pub fn record<'a, I>(
        &mut self,
        inputs: I,
        signature: &Signature,
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

    /// See `Session::record_server`.
    pub fn record_server(
        &mut self,
        input: ServerInput,
        signature: &Signature,
    ) -> Result<(), ServerInputRefused> {
        self.world
            .resource_mut::<Session>()
            .record_server(input, signature)
    }

    pub fn run_tick(&mut self) {
        Session::run_tick(&mut self.world);
    }

    /// See `Session::keep_journal`.
    pub fn keep_journal(&mut self, journal: Journal) {
        self.world.resource_mut::<Session>().keep_journal(journal);
    }

    /// See `Session::checkpoint`.
    pub fn checkpoint(&self, snapshot: &mut Vec<u8>) -> Option<Checkpoint> {
        self.world
            .resource::<Session>()
            .checkpoint(&self.world, snapshot)
    }

    /// See `Session::record_checkpoint`.
    pub fn record_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        self.world
            .resource_mut::<Session>()
            .record_checkpoint(record, signature)
    }

    /// The result that ends the session before the next tick, as the mode ended the match or
    /// aborted when it did not; see `Session::result`.
    pub fn result(&self) -> SessionResult {
        self.result_as(Session::outcome(&self.world))
    }

    /// See `Session::result`.
    pub fn result_as(&self, outcome: Outcome) -> SessionResult {
        self.world
            .resource::<Session>()
            .result(&self.world, outcome)
    }

    /// See `Session::record_result`.
    pub fn record_result(
        &mut self,
        result: SessionResult,
        signature: &Signature,
    ) -> Result<(), ResultError> {
        self.world
            .resource_mut::<Session>()
            .record_result(result, signature)
    }

    /// See `Session::check_result`.
    pub fn check_result(&self) -> Result<(), ResultMismatch> {
        self.world.resource::<Session>().check_result(&self.world)
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

#[cfg(feature = "bench")]
pub(crate) mod bench;

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::runner::Runner;

    impl Runner {
        /// The match's world, for a test that changes what no input can.
        pub const fn world_mut(&mut self) -> &mut World {
            &mut self.world
        }
    }
}
