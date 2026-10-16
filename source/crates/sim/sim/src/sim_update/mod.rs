use bevy_ecs::schedule::{
    IntoScheduleConfigs, Schedule, ScheduleLabel, SingleThreadedExecutor, SystemSet,
};
use bevy_ecs::system::{Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::{EnumAll, SegmentSeed};
use campfire_schedule::StrictSchedule;

use crate::id_allocator::IdAllocator;
use crate::sim_rng::SimRng;
use crate::sim_tick::SimTick;
use crate::stable_id::entity_index::EntityIndex;
use crate::tick_inputs::TickInputs;
use crate::tick_rate::TickRate;

/// The schedule that runs one sim tick. The server and client run it from Lightyear's fixed
/// update; the verifier runs it in a bare `World`.
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimUpdate;

/// The stages of a tick, in this order. Each capability puts its systems into them, and orders
/// them within a stage against the capabilities it builds on.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
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

/// The close of a tick, after its last gap: the tick advances, its inputs clear, and the records
/// a tick keeps for the next age.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TickEnd;

impl SimUpdate {
    /// The schedule with no game systems yet: the stages in order, each with its `SimEdge` after
    /// it, and `SimEdge::Start` before the first. The tick's random sequences start before
    /// `SimEdge::Start`; after `SimEdge::After(SimSet::Vision)`, in `TickEnd`, the tick advances,
    /// its inputs are cleared and its removal records age, so an edge's systems read the tick's
    /// random sequences and inputs. It is a `StrictSchedule`, so an ambiguity or a redundant edge
    /// fails every test that builds it, not only as a log line of a running match. It runs on one
    /// thread whatever features the build turns on: a tick's systems are too small to share, and
    /// Bevy's parallel executor, which its `multi_threaded` feature turns on wherever a workspace
    /// build enables it, made a 3v3 tick cost 2.5 times as much.
    pub(crate) fn schedule() -> Schedule {
        let mut schedule = StrictSchedule::of(Self);
        schedule.set_executor(SingleThreadedExecutor::new());
        schedule.configure_sets(SimEdge::Start.before(SimSet::Inputs));
        for stages in SimSet::ALL.windows(2) {
            schedule.configure_sets((
                stages[1].after(stages[0]),
                SimEdge::After(stages[0]).after(stages[0]).before(stages[1]),
            ));
        }
        schedule.configure_sets((
            SimEdge::After(SimSet::Vision).after(SimSet::Vision),
            TickEnd.after(SimEdge::After(SimSet::Vision)),
        ));
        schedule.add_systems((start_tick.before(SimEdge::Start), end_tick.in_set(TickEnd)));
        schedule
    }

    /// Inserts what the schedule needs into a new world: the tick at 0, the tick `rate`, the
    /// random sequences of `seed`, the id allocator, the entity index and empty tick inputs. A
    /// restore then replaces the state among them.
    pub(crate) fn prepare(world: &mut World, seed: SegmentSeed, rate: TickRate) {
        world.insert_resource(SimTick::default());
        world.insert_resource(rate);
        world.insert_resource(SimRng::new(seed));
        world.insert_resource(IdAllocator::default());
        world.insert_resource(EntityIndex::default());
        world.insert_resource(TickInputs::default());
    }
}

pub(crate) fn start_tick(tick: Res<'_, SimTick>, mut rng: ResMut<'_, SimRng>) {
    rng.begin_tick(*tick);
}

pub(crate) fn end_tick(mut tick: ResMut<'_, SimTick>, mut inputs: ResMut<'_, TickInputs>) {
    tick.advance();
    inputs.clear();
}

pub(crate) mod sim_place;
#[cfg(feature = "internals")]
pub(crate) mod stage_clock;

#[cfg(test)]
mod tests;
