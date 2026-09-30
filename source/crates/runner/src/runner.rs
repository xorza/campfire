use bevy_ecs::world::World;
use campfire_kit_moba::{MobaKit, MoveStep};
use campfire_math::{Num, Vec3};
use campfire_protocol::{
    Applied, InputError, PlayerInput, SeedError, ServerSeed, SessionHeader, SessionLog,
};
use campfire_sim::{Position, SimTick, SimUpdate, StateHash, StateRegistry, TickInput, TickInputs};

/// A quarter meter, 7.5 m/s at the MOBA's default 30 ticks a second.
const HERO_STEP: Num = Num::from_bits(1 << (Num::FRAC_BITS - 2));

/// A match in a bare `World`: the session log, and the sim with the MOBA kit, run one tick at a
/// time. The server records inputs as they arrive; a verifier records a published log's inputs
/// again. Either way each tick applies exactly the inputs the log gives it.
///
/// Until modes load from packages, a match is one hero per player in the header, all at the
/// origin.
#[derive(Debug)]
pub struct Runner {
    /// Secret until `reveal_seed` publishes the log.
    server_seed: ServerSeed,
    world: World,
    state: StateRegistry,
    log: SessionLog,
}

impl Runner {
    /// The match of `header`, with the randomness of `server_seed` and the players'
    /// contributions; an error when `server_seed` is not the one the header commits to.
    pub fn new(header: SessionHeader, server_seed: ServerSeed) -> Result<Runner, SeedError> {
        let seed = header.segment_seed(&server_seed)?;
        let mut world = World::new();
        SimUpdate::prepare(&mut world, seed);
        let mut schedule = SimUpdate::schedule();
        MobaKit::add_systems(&mut schedule);
        world.add_schedule(schedule);
        let mut state = StateRegistry::new();
        MobaKit::register_state(&mut state);

        let origin = Position::new(Vec3::ZERO).expect("the origin is within the bound");
        let step = MoveStep::new(HERO_STEP).expect("the hero step is not negative");
        for slot in 0..header.players.len() {
            let slot = u32::try_from(slot).expect("player slots fit u32");
            MobaKit::spawn_hero(&mut world, slot, origin, step);
        }
        Ok(Runner {
            server_seed,
            world,
            state,
            log: SessionLog::new(header),
        })
    }

    /// Logs `input` before the next tick; see `SessionLog::record`.
    pub fn record(&mut self, input: PlayerInput<'_>) -> Result<Applied, InputError> {
        self.log.record(input)
    }

    /// Seals the next tick in the log and runs it with the inputs applied in it.
    pub fn run_tick(&mut self) {
        debug_assert_eq!(
            self.world.resource::<SimTick>().get(),
            self.log.next_tick(),
            "the sim and the log are at the same tick"
        );
        let mut inputs = self.world.resource_mut::<TickInputs>();
        for input in self.log.seal_tick() {
            inputs.push(TickInput {
                slot: input.slot.get(),
                payload: input.payload,
            });
        }
        self.world.run_schedule(SimUpdate);
    }

    /// Publishes the log's segment by adding the server seed.
    pub fn reveal_seed(&mut self) {
        self.log.reveal_seed(self.server_seed);
    }

    pub fn state_hash(&self) -> StateHash {
        self.state.hash(&self.world)
    }

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }

    pub const fn world(&self) -> &World {
        &self.world
    }
}
