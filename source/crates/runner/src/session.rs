use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_kit_moba::{MobaKit, MoveStep};
use campfire_math::{Num, Vec3};
use campfire_protocol::{
    Applied, ChainSignature, InputError, PlayerInput, SeedError, ServerSeed, SessionLog,
};
use campfire_sim::{Position, SimTick, SimUpdate, StateHash, StateRegistry, TickInput, TickInputs};

/// A quarter meter, 7.5 m/s at the MOBA's default 30 ticks a second.
const HERO_STEP: Num = Num::from_bits(1 << (Num::FRAC_BITS - 2));

/// A match's session log and state types, kept as a resource in the `World` that runs the match:
/// a bare one on a verifier, Lightyear's on a server. The server records inputs as they arrive; a
/// verifier records a published log's inputs again. Either way each tick applies exactly the
/// inputs the log gives it.
///
/// Until modes load from packages, a match is one hero per player in the header, all at the
/// origin.
#[derive(Resource, Debug)]
pub struct Session {
    /// Secret until `reveal_seed` publishes the log.
    server_seed: ServerSeed,
    state: StateRegistry,
    log: SessionLog,
}

impl Session {
    /// Prepares `world` for the match of `log`'s header, with the randomness of `server_seed` and
    /// the players' contributions, and inserts the session, which records into `log` from its
    /// first tick; an error when `server_seed` is not the one the header commits to.
    pub fn start(
        world: &mut World,
        log: SessionLog,
        server_seed: ServerSeed,
    ) -> Result<(), SeedError> {
        assert_eq!(log.next_tick(), 0, "a session starts before its first tick");
        let header = log.header();
        let seed = header.segment_seed(&server_seed)?;
        SimUpdate::prepare(world, seed);
        let mut schedule = SimUpdate::schedule();
        MobaKit::add_systems(&mut schedule);
        world.add_schedule(schedule);
        let mut state = StateRegistry::new();
        MobaKit::register_state(&mut state);

        let origin = Position::new(Vec3::ZERO).expect("the origin is within the bound");
        let step = MoveStep::new(HERO_STEP).expect("the hero step is not negative");
        for slot in 0..header.players.len() {
            let slot = u32::try_from(slot).expect("player slots fit u32");
            MobaKit::spawn_hero(world, slot, origin, step);
        }
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
