use bevy_ecs::bundle::Bundle;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;
use campfire_capabilities::{
    AttackStats, Combat, Combatant, Control, Controller, Health, LaneWalker, Lanes, MoveStep,
    Navigation, OnDeath, PathDirection, Team, TowerAi,
};
use campfire_math::{Num, Vec3};
use campfire_sim::{IdAllocator, Position, SimSet, SimTick, StateRegistry};

/// A hero strikes 1.25 m away 8 ticks into an attack every 20, and stays when it dies.
const HERO: Combatant = Combatant {
    health: health(600),
    attack: attack(quarters(5), 8, 20, 60),
    on_death: OnDeath::Stay,
};
/// A quarter meter a tick, 7.5 m/s at the MOBA's default 30 ticks a second.
const HERO_STEP: MoveStep = step(quarters(1));
/// A tower strikes 7.75 m away 5 ticks into an attack every 37: 150 ms and 0.83 attacks a
/// second, rounded up to whole ticks.
const TOWER: Combatant = Combatant {
    health: health(1500),
    attack: attack(quarters(31), 5, 37, 150),
    on_death: OnDeath::Despawn,
};
/// Until creep AI runs, a creep never attacks.
const CREEP: Combatant = Combatant {
    health: health(445),
    attack: attack(quarters(4), 9, 24, 12),
    on_death: OnDeath::Despawn,
};
/// An eighth of a meter a tick.
const CREEP_STEP: MoveStep = step(Num::from_bits(1 << (Num::FRAC_BITS - 3)));
/// The one lane, along x through the origin, from the first side's end to the second's.
const LANE: [Position; 3] = [at(-16), at(0), at(16)];
/// The two sides: the first walks the lane forward from its end at x = −16, the second backward.
const SIDES: [(Team, PathDirection); 2] = [
    (Team::new(0), PathDirection::Forward),
    (Team::new(1), PathDirection::Backward),
];
const TOWERS: [(Team, Position); 2] = [(SIDES[0].0, at(-8)), (SIDES[1].0, at(8))];
/// Creeps each side gets in a wave, from tick 0 and then every `WAVE_INTERVAL` ticks: 30 s.
const WAVE_CREEPS: usize = 2;
const WAVE_INTERVAL: u64 = 900;

/// The match every session plays until modes load from packages, which will replace this file:
/// `combat`, `navigation` and `control` on one lane; one hero per player, all at the origin, even
/// slots on the first side and odd on the second; a tower a side 8 m down the lane, just out of
/// reach of the origin; and creep waves from each end, on a timer standing in for the mode
/// script's.
#[derive(Debug)]
pub(crate) struct StandInMode;

impl StandInMode {
    /// Installs the mode's capabilities and its map into `world`, which `SimUpdate::prepare` set
    /// up, and adds its wave timer in the Mode stage.
    pub(crate) fn install(world: &mut World, schedule: &mut Schedule, state: &mut StateRegistry) {
        Combat::install(world, schedule, state);
        Navigation::install(world, schedule, state);
        Control::install(schedule, state);
        world.insert_resource(Lanes::new([&LANE[..]]));
        schedule.add_systems(spawn_waves.in_set(SimSet::Mode));
    }

    /// Spawns the heroes of `players` players and the towers.
    pub(crate) fn start(world: &mut World, players: usize) {
        for slot in 0..players {
            let slot = u32::try_from(slot).expect("player slots fit u32");
            let (team, _) = SIDES[slot as usize % 2];
            spawn(
                world,
                at(0),
                (HERO.bundle(team), HERO_STEP.bundle(), Controller::new(slot)),
            );
        }
        for (team, position) in TOWERS {
            spawn(world, position, (TOWER.bundle(team), TowerAi));
        }
    }
}

/// Spawns a wave in each due tick: on each lane, the first side's creeps at its end, then the
/// second side's at theirs.
fn spawn_waves(world: &mut World) {
    if !world
        .resource::<SimTick>()
        .get()
        .is_multiple_of(WAVE_INTERVAL)
    {
        return;
    }
    for lane in 0..world.resource::<Lanes>().count() {
        for (team, direction) in SIDES {
            let start = world
                .resource::<Lanes>()
                .waypoint(lane, 0, direction)
                .expect("a lane has a waypoint");
            for _ in 0..WAVE_CREEPS {
                let creep = (
                    CREEP.bundle(team),
                    CREEP_STEP.bundle(),
                    LaneWalker::start(lane, direction),
                );
                spawn(world, start, creep);
            }
        }
    }
}

fn spawn(world: &mut World, at: Position, parts: impl Bundle) {
    let id = world.resource_mut::<IdAllocator>().allocate();
    world.spawn((id, at, parts));
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

#[cfg(test)]
mod tests {
    use campfire_math::SegmentSeed;
    use campfire_sim::{EntityIndex, SimUpdate};

    use super::*;

    #[test]
    fn waves_come_every_900_ticks() {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
        let mut schedule = SimUpdate::schedule();
        StandInMode::install(&mut world, &mut schedule, &mut StateRegistry::new());
        world.add_schedule(schedule);
        StandInMode::start(&mut world, 1);
        let last_id = |world: &World| {
            let (id, _) = world.resource::<EntityIndex>().iter().last().unwrap();
            id.get()
        };
        // The hero 0 and the towers 1 and 2, then 4 creeps a wave: ids 3 to 6 in tick 0, and
        // 7 to 10 in tick 900.
        assert_eq!(last_id(&world), 2);
        world.run_schedule(SimUpdate);
        assert_eq!(last_id(&world), 6);
        while world.resource::<SimTick>().get() < 900 {
            world.run_schedule(SimUpdate);
        }
        assert_eq!(last_id(&world), 6);
        world.run_schedule(SimUpdate);
        assert_eq!(last_id(&world), 10);
    }
}
