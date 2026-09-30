use std::num::NonZeroU64;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_kit_moba::{AttackStats, Health, Lanes, MobaKit, MoveStep, Team, UnitStats, Waves};
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StateRegistry};

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

/// The match every session plays until modes load from packages, which will replace this file:
/// the MOBA kit on one lane, one hero per player, all at the origin, even slots on the first side
/// and odd on the second; a tower a side 8 m down the lane, just out of reach of the origin; and
/// creep waves from each end.
#[derive(Debug)]
pub(crate) struct StandInMode;

impl StandInMode {
    /// Adds the mode's kits to a match's tick and state types.
    pub(crate) fn add_kits(schedule: &mut Schedule, state: &mut StateRegistry) {
        MobaKit::add_systems(schedule);
        MobaKit::register_state(state);
    }

    /// Sets up the match of `players` players in `world`, which `SimUpdate::prepare` set up.
    pub(crate) fn start(world: &mut World, players: usize) {
        let waves = Waves {
            first: 0,
            interval: WAVE_INTERVAL,
            creeps: vec![CREEP; 2],
        };
        MobaKit::prepare(world, Lanes::new([&LANE[..]]), Some(waves));
        for slot in 0..players {
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
