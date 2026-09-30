use std::collections::BTreeMap;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;
use campfire_capabilities::{
    AiData, AttackStats, Combat, Combatant, Control, Controller, Health, LaneWalker, Lanes,
    MoveStep, Navigation, OnDeath, PathDirection, Projectiles, Scalar, Team, UnitType,
    UnitTypeData, Units,
};
use campfire_content::PackagePath;
use campfire_math::{Num, Vec3};
use campfire_script::ScriptLimits;
use campfire_sim::{IdAllocator, Position, SimSet, SimTick, StateRegistry, TickRate};

/// A hero strikes 1.25 m away 8 ticks into an attack every 20, and stays when it dies.
const HERO: Combatant = Combatant {
    health: health(600),
    attack: attack(quarters(5), 8, 20, 60),
    on_death: OnDeath::Stay,
};
/// A quarter meter a tick, 7.5 m/s at 30 ticks a second.
const HERO_STEP: MoveStep = step(quarters(1));
/// A tower strikes 7.75 m away 5 ticks into an attack every 37: 150 ms and 0.83 attacks a
/// second, rounded up to whole ticks. Its projectile flies 12 m/s.
const TOWER: Combatant = Combatant {
    health: health(1500),
    attack: ranged(attack(quarters(31), 5, 37, 150), per_tick(12)),
    on_death: OnDeath::Despawn,
};
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
const CREEP_AI: &str = include_str!("../../../packages/moba/modes/3v3/scripts/creep_ai.rhai");
const TOWER_AI: &str = include_str!("../../../packages/moba/modes/3v3/scripts/tower_ai.rhai");
/// As the 3v3 package's `units.toml` declares them.
const THINK_MS: u64 = 250;
const HELP_WINDOW_MS: i64 = 2000;

/// The match every session plays until modes load from packages, which will replace this file:
/// the core, `combat`, `navigation`, `projectiles` and `control` on one lane; one hero per
/// player, all at the origin, even slots on the first side and odd on the second; a tower a side
/// 8 m down the lane, just out of reach of the origin; and creep waves from each end, on a timer
/// standing in for the mode script's. Creeps and towers think with the 3v3 package's AI scripts.
#[derive(Debug)]
pub struct StandInMode;

/// The unit types of the stand-in match.
#[derive(Resource, Debug, Clone, Copy)]
struct StandInTypes {
    hero: UnitType,
    creep: UnitType,
    tower: UnitType,
}

impl StandInMode {
    /// 30 ticks a second, the MOBA's default.
    pub const TICK_RATE: TickRate = TickRate::new(30).expect("a positive rate");
    /// Script operations a call, and a tick, may run.
    pub const SCRIPT_LIMITS: ScriptLimits = ScriptLimits {
        per_call: 20_000,
        per_tick: 200_000,
    };

    /// Installs the mode's capabilities, unit types and map into `world`, which
    /// `SimUpdate::prepare` set up, and adds its wave timer in the Mode stage.
    pub(crate) fn install(world: &mut World, schedule: &mut Schedule, state: &mut StateRegistry) {
        Units::install(
            world,
            schedule,
            state,
            StandInMode::SCRIPT_LIMITS,
            StandInMode::TICK_RATE,
        );
        Combat::install(world, schedule, state);
        Navigation::install(world, schedule, state);
        Projectiles::install(world, schedule, state);
        Control::install(world, schedule, state);
        world.insert_resource(Lanes::new([&LANE[..]]));
        let types = StandInTypes::load(world);
        world.insert_resource(types);
        schedule.add_systems(spawn_waves.in_set(SimSet::Mode));
    }

    /// Spawns the heroes of `players` players and the towers.
    pub(crate) fn start(world: &mut World, players: usize) {
        let types = *world.resource::<StandInTypes>();
        for slot in 0..players {
            let slot = u32::try_from(slot).expect("player slots fit u32");
            let (team, _) = SIDES[slot as usize % 2];
            let hero = (HERO.bundle(team), HERO_STEP.bundle(), Controller::new(slot));
            spawn(world, at(0), (types.hero, hero));
        }
        for (team, position) in TOWERS {
            spawn(world, position, (types.tower, TOWER.bundle(team)));
        }
    }
}

impl StandInTypes {
    /// Loads the hero, creep and tower types, the creep's and the tower's with their AI.
    fn load(world: &mut World) -> StandInTypes {
        let help_window = ("help_window_ms", Scalar::Int(HELP_WINDOW_MS));
        let meters = |value| Scalar::Decimal(Num::from_int(value).expect("a few meters"));
        let mut load = |tags: &[&str], params: &[(&str, Scalar)], ai: Option<(&str, &str)>| {
            let data = UnitTypeData {
                tags: tags.iter().map(|&tag| tag.to_owned()).collect(),
                params: params
                    .iter()
                    .map(|&(name, value)| (name.to_owned(), value))
                    .collect::<BTreeMap<_, _>>(),
            };
            let unit_type = Units::load_type(world, &data).expect("the stand-in's types load");
            if let Some((path, source)) = ai {
                let ai = AiData {
                    ai: PackagePath::parse(path).expect("a path in the package"),
                    think_ms: THINK_MS,
                };
                Control::load_ai(world, unit_type, &ai, source).expect("the AI scripts compile");
            }
            unit_type
        };
        let hero = load(&["hero"], &[], None);
        let creep = load(
            &["creep"],
            &[
                ("aggro_range", meters(7)),
                ("help_range", meters(5)),
                help_window,
            ],
            Some(("scripts/creep_ai.rhai", CREEP_AI)),
        );
        let tower = load(
            &["structure", "tower"],
            &[help_window],
            Some(("scripts/tower_ai.rhai", TOWER_AI)),
        );
        StandInTypes { hero, creep, tower }
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
    let creep_type = world.resource::<StandInTypes>().creep;
    for lane in 0..world.resource::<Lanes>().count() {
        for (team, direction) in SIDES {
            let start = world
                .resource::<Lanes>()
                .waypoint(lane, 0, direction)
                .expect("a lane has a waypoint");
            for _ in 0..WAVE_CREEPS {
                let creep = (
                    creep_type,
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

/// `meters` a second, as a distance a tick at the mode's rate.
const fn per_tick(meters: i64) -> Num {
    Num::from_bits(meters << Num::FRAC_BITS)
        .checked_div_int(StandInMode::TICK_RATE.hz() as i64)
        .expect("a speed a tick fits a Num")
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

const fn ranged(melee: AttackStats, speed: Num) -> AttackStats {
    melee.ranged(speed).expect("a positive speed")
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
        // A wave spawns in the Mode stage, after units move: at the end of its tick, its creeps
        // stand at the lane's ends, 2 a side; a tick later they walked on.
        let at_ends = |world: &World| {
            let creep = world.resource::<StandInTypes>().creep;
            let mut ends = [0, 0];
            for (_, entity) in world.resource::<EntityIndex>().iter() {
                let unit = world.entity(entity);
                if unit.get::<UnitType>() != Some(&creep) {
                    continue;
                }
                let position = *unit.get::<Position>().unwrap();
                for (end, x) in [(0, -16), (1, 16)] {
                    if position == at(x) {
                        ends[end] += 1;
                    }
                }
            }
            ends
        };
        let mut waves = Vec::new();
        while world.resource::<SimTick>().get() <= 901 {
            world.run_schedule(SimUpdate);
            let ended = world.resource::<SimTick>().get() - 1;
            if at_ends(&world) != [0, 0] {
                waves.push((ended, at_ends(&world)));
            }
        }
        assert_eq!(waves, [(0, [2, 2]), (900, [2, 2])]);
    }
}
