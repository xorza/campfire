use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_protocol::{Applied, ChainSignature, InputError, PlayerInput, ServerSeed, SessionLog};
use campfire_sim::{SimTick, SimUpdate, StateHash, StateRegistry, TickInput, TickInputs, TickRate};

use crate::error::StartError;
use crate::stand_in_mode::StandInMode;

/// A match's session log and state types, kept as a resource in the `World` that runs the match:
/// a bare one on a verifier, Lightyear's on a server. The server records inputs as they arrive; a
/// verifier records a published log's inputs again. Either way each tick applies exactly the
/// inputs the log gives it. Until modes load from packages, every match is `StandInMode`'s.
#[derive(Resource, Debug)]
pub struct Session {
    /// The first segment's server seed, secret until `reveal_seed` publishes the log.
    server_seed: ServerSeed,
    state: StateRegistry,
    log: SessionLog,
}

impl Session {
    /// Prepares `world` for the match of `log`'s header, at its tick rate and with the
    /// randomness of `server_seed`, the first segment's, and the players' contributions, and
    /// inserts the session, which records into `log` from its first tick; an error when
    /// `server_seed` is not the first segment's seed of the chain the header commits to, or the
    /// mode does not run at the header's rate.
    pub fn start(
        world: &mut World,
        log: SessionLog,
        server_seed: ServerSeed,
    ) -> Result<(), StartError> {
        assert_eq!(log.next_tick(), 0, "a session starts before its first tick");
        let header = log.header();
        let hz = header.terms.tick_hz;
        if hz != StandInMode::TICK_HZ {
            return Err(StartError::TickRate(hz));
        }
        let seed = header
            .segment_seed(0, &server_seed)
            .map_err(StartError::Seed)?;
        SimUpdate::prepare(world, seed, TickRate::new(hz));
        let mut schedule = SimUpdate::schedule();
        let mut state = StateRegistry::new();
        StandInMode::install(world, &mut schedule, &mut state, header.players.len());
        world.add_schedule(schedule);
        StandInMode::start(world, header.players.len());
        world.insert_resource(Session {
            server_seed,
            state,
            log,
        });
        Ok(())
    }

    /// Logs a player's packet before the next tick; see `SessionLog::record`.
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
        self.log.record(inputs, signature, applied)
    }

    /// Seals the next tick in the log of the session in `world` and runs it with the inputs
    /// applied in it.
    pub fn run_tick(world: &mut World) {
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            debug_assert_eq!(
                world.resource::<SimTick>().get(),
                session.log.next_tick(),
                "the sim and the log are at the same tick"
            );
            let mut inputs = world.resource_mut::<TickInputs>();
            for input in session.log.seal_tick() {
                inputs.push(TickInput {
                    slot: input.slot.get(),
                    payload: input.payload,
                });
            }
        });
        world.run_schedule(SimUpdate);
    }

    /// Publishes the log's segment by adding the server seed.
    pub fn reveal_seed(&mut self) {
        self.log.reveal_seed(self.server_seed);
    }

    pub fn state_hash(&self, world: &World) -> StateHash {
        self.state.hash(world)
    }

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }
}
