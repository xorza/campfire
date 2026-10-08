use bevy_ecs::schedule::{
    IntoScheduleConfigs, LogLevel, Schedule, ScheduleBuildSettings, ScheduleLabel,
    SingleThreadedExecutor, SystemSet,
};
use bevy_ecs::system::{Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::SegmentSeed;

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::sim_rng::SimRng;
use crate::sim_tick::SimTick;
use crate::tick_inputs::TickInputs;
use crate::tick_rate::TickRate;

/// The schedule that runs one sim tick. The server and client run it from Lightyear's fixed
/// update; the verifier runs it in a bare `World`.
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimUpdate;

/// The stages of a tick, in this order. Each capability puts its systems into them, and orders
/// them within a stage against the capabilities it builds on.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimSet {
    /// The tick's commands reach the capabilities that own them.
    Inputs,
    /// AI of the units due this tick issues orders.
    Think,
    /// Current orders run: attack windups, cast starts, path requests.
    Act,
    /// Units move.
    Move,
    /// The mode's collision backend resolves overlaps.
    Collide,
    /// Strikes, hits, projectiles and areas.
    Hit,
    /// Damage and modifiers apply, and units die.
    Resolve,
    /// Due timers, the capabilities' events, spawns.
    Mode,
    /// The mode's vision backend marks what each team may see.
    Vision,
}

impl SimSet {
    /// Every stage, in the order a tick runs them.
    pub const ALL: [SimSet; 9] = [
        SimSet::Inputs,
        SimSet::Think,
        SimSet::Act,
        SimSet::Move,
        SimSet::Collide,
        SimSet::Hit,
        SimSet::Resolve,
        SimSet::Mode,
        SimSet::Vision,
    ];
}

/// The gaps of a tick around its stages, for a pass over what came before, such as values derived
/// again from what the stage before changed. A stage's time, as `StageClock` measures it, holds
/// the gap after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimEdge {
    /// Before the first stage: work on what changed between ticks.
    Start,
    /// After a stage, and before the next: work on what that stage changed.
    After(SimSet),
}

impl SimUpdate {
    /// The schedule with no game systems yet: the stages in order, each with its `SimEdge` after
    /// it, and `SimEdge::Start` before the first. The tick's random sequences start before
    /// `SimEdge::Start`; after `SimEdge::After(SimSet::Vision)` the tick advances and its inputs
    /// are cleared, so an edge's systems read the tick's random sequences and inputs. Two
    /// systems with conflicting access and no order fail the build, since either order could win.
    /// A set membership that a longer path already implies fails it too, so the redundant edge
    /// shows in every test that builds the schedule, not only as a log line of a running match.
    /// It runs on one thread whatever features the build turns on: a tick's systems are too small
    /// to share, and Bevy's parallel executor, which its `multi_threaded` feature turns on
    /// wherever a workspace build enables it, made a 3v3 tick cost 2.5 times as much.
    pub fn schedule() -> Schedule {
        let mut schedule = Schedule::new(SimUpdate);
        schedule.set_executor(SingleThreadedExecutor::new());
        #[expect(
            clippy::disallowed_methods,
            reason = "the one place that sets the build settings a match runs with"
        )]
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            hierarchy_detection: LogLevel::Error,
            ..ScheduleBuildSettings::new()
        });
        schedule.configure_sets(SimEdge::Start.before(SimSet::Inputs));
        for stages in SimSet::ALL.windows(2) {
            schedule.configure_sets((
                stages[1].after(stages[0]),
                SimEdge::After(stages[0]).after(stages[0]).before(stages[1]),
            ));
        }
        schedule.configure_sets(SimEdge::After(SimSet::Vision).after(SimSet::Vision));
        schedule.add_systems((
            start_tick.before(SimEdge::Start),
            end_tick.after(SimEdge::After(SimSet::Vision)),
        ));
        schedule
    }

    /// Inserts what the schedule needs into a new world: the tick at 0, the tick `rate`, the
    /// random sequences of `seed`, the id allocator, the entity index and empty tick inputs. A
    /// restore then replaces the state among them.
    pub fn prepare(world: &mut World, seed: SegmentSeed, rate: TickRate) {
        world.insert_resource(SimTick::default());
        world.insert_resource(rate);
        world.insert_resource(SimRng::new(seed));
        world.insert_resource(IdAllocator::default());
        world.insert_resource(EntityIndex::default());
        world.insert_resource(TickInputs::default());
    }
}

fn start_tick(tick: Res<'_, SimTick>, mut rng: ResMut<'_, SimRng>) {
    rng.begin_tick(*tick);
}

fn end_tick(mut tick: ResMut<'_, SimTick>, mut inputs: ResMut<'_, TickInputs>) {
    tick.advance();
    inputs.clear();
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod stage_clock;

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::collections::BTreeSet;

    use bevy_ecs::schedule::{IntoSystemSet, ScheduleBuildSettings, SystemSet};
    use bevy_ecs::world::World;

    use crate::sim_update::{SimEdge, SimSet, SimUpdate, end_tick, start_tick};

    impl SimUpdate {
        /// Builds `world`'s sim schedule again with no automatic sync points, and the build's
        /// other settings kept: one of them orders the systems it lies between, so the ambiguity
        /// check then sees every pair of systems that only a sync point keeps in order. The error
        /// names its systems.
        pub fn build_without_sync_points(world: &mut World) -> Result<(), String> {
            world.schedule_scope(SimUpdate, |world, schedule| {
                let settings = ScheduleBuildSettings {
                    auto_insert_apply_deferred: false,
                    ..schedule.get_build_settings()
                };
                #[expect(clippy::disallowed_methods, reason = "turns only the sync points off")]
                schedule.set_build_settings(settings);
                let built = schedule.initialize(world).map(drop);
                built.map_err(|error| error.to_string(schedule.graph(), world))
            })
        }

        /// The name of each system of `world`'s sim schedule that sits in no stage and no
        /// `SimEdge`, but the tick's own start and end. Such a system runs in a gap between two
        /// stages, in an order Bevy picks, so a bench's probes time it with either stage. It
        /// builds the schedule as `build_without_sync_points` does, so a sync point it finds is
        /// one the code added.
        pub fn systems_outside_stages(world: &mut World) -> Vec<String> {
            if let Err(error) = SimUpdate::build_without_sync_points(world) {
                panic!("{error}");
            }
            world.schedule_scope(SimUpdate, |_, schedule| {
                let graph = schedule.graph();
                let sets = SimSet::ALL
                    .into_iter()
                    .flat_map(|stage| [stage.intern(), SimEdge::After(stage).intern()])
                    .chain([
                        SimEdge::Start.intern(),
                        start_tick.into_system_set().intern(),
                        end_tick.into_system_set().intern(),
                    ]);
                let mut placed = BTreeSet::new();
                for set in sets {
                    let systems = graph
                        .systems_in_set(set)
                        .expect("a set of the built sim schedule");
                    placed.extend(systems.iter().copied());
                }
                schedule
                    .systems()
                    .expect("the schedule is built")
                    .filter(|&(key, _)| !placed.contains(&key))
                    .map(|(_, system)| system.name().shortname().to_string())
                    .collect()
            })
        }
    }
}

#[cfg(test)]
mod tests;
