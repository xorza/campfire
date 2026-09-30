use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use std::num::NonZeroU64;

use campfire_kit_moba::{AttackStats, Health, Lanes, MobaKit, MoveStep, Team, UnitStats, Waves};
use campfire_math::{Num, Vec3};
use campfire_protocol::{
    Applied, ChainSignature, InputError, PlayerInput, SeedError, ServerSeed, SessionLog,
};
use campfire_sim::{Position, SimTick, SimUpdate, StateHash, StateRegistry, TickInput, TickInputs};

/// A hero walks a quarter meter a tick, 7.5 m/s at the MOBA's default 30 ticks a second, and
/// strikes 1.25 m away 8 ticks into an attack every 20.
const HERO: UnitStats = UnitStats {
    health: health(600),
    attack: attack(quarters(5), 8, 20, 60),
    step: Some(step(quarters(1))),
};
/// A tower strikes 7.75 m away 5 ticks into an attack every 37: 150 ms and 0.83 attacks a
/// second, rounded up to whole ticks.
const TOWER: UnitStats = UnitStats {
    health: health(1500),
    attack: attack(quarters(31), 5, 37, 150),
    step: None,
};
/// A creep walks an eighth of a meter a tick. Until creep AI runs, it never attacks.
const CREEP: UnitStats = UnitStats {
    health: health(445),
    attack: attack(quarters(4), 9, 24, 12),
    step: Some(step(quarters(1).checked_div_int(2).expect("an eighth"))),
};
/// The one lane, along x through the origin, from the first side's end to the second's.
const LANE: [Position; 3] = [at(-16), at(0), at(16)];
const TOWERS: [(Team, Position); 2] = [(Team::First, at(-8)), (Team::Second, at(8))];
/// Two creeps a side from tick 0, then every 900 ticks: 30 s.
const WAVE_INTERVAL: NonZeroU64 = NonZeroU64::new(900).expect("not zero");

/// A match's session log and state types, kept as a resource in the `World` that runs the match:
/// a bare one on a verifier, Lightyear's on a server. The server records inputs as they arrive; a
/// verifier records a published log's inputs again. Either way each tick applies exactly the
/// inputs the log gives it.
///
/// Until modes load from packages, a match is a stand-in lane: one hero per player in the header,
/// all at the origin, even slots on the first side and odd on the second; a tower a side 8 m
/// down the lane, just out of reach of the origin; and creep waves from each end.
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
        let waves = Waves {
            first: 0,
            interval: WAVE_INTERVAL,
            creeps: vec![CREEP; 2],
        };
        MobaKit::prepare(world, Lanes::new([&LANE[..]]), Some(waves));
        let mut schedule = SimUpdate::schedule();
        MobaKit::add_systems(&mut schedule);
        world.add_schedule(schedule);
        let mut state = StateRegistry::new();
        MobaKit::register_state(&mut state);

        for slot in 0..header.players.len() {
            let slot = u32::try_from(slot).expect("player slots fit u32");
            let team = if slot % 2 == 0 {
                Team::First
            } else {
                Team::Second
            };
            MobaKit::spawn_hero(world, slot, team, at(0), HERO);
        }
        for (team, position) in TOWERS {
            MobaKit::spawn_tower(world, team, position, TOWER);
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

const fn quarters(count: i64) -> Num {
    Num::from_bits(count << (Num::FRAC_BITS - 2))
}

const fn health(max: i64) -> Health {
    Health::new(Num::from_bits(max << Num::FRAC_BITS)).expect("positive health")
}

const fn attack(range: Num, windup: u32, period: u32, damage: i64) -> AttackStats {
    AttackStats::new(
        range,
        windup,
        period,
        Num::from_bits(damage << Num::FRAC_BITS),
    )
    .expect("attack stats within their limits")
}

const fn step(meters: Num) -> MoveStep {
    MoveStep::new(meters).expect("a step is not negative")
}

/// `x` meters along the lane.
const fn at(x: i64) -> Position {
    Position::new(Vec3::new(
        Num::from_bits(x << Num::FRAC_BITS),
        Num::ZERO,
        Num::ZERO,
    ))
    .expect("within the bound")
}
