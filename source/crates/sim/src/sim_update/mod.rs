use bevy_ecs::schedule::{
    IntoScheduleConfigs, LogLevel, Schedule, ScheduleBuildSettings, ScheduleLabel, SystemSet,
};
use bevy_ecs::system::{Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::SegmentSeed;

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::sim_rng::SimRng;
use crate::sim_tick::SimTick;
use crate::tick_inputs::TickInputs;

/// The schedule that runs one sim tick. The server and client run it from Lightyear's fixed
/// update; the verifier runs it in a bare `World`.
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimUpdate;

/// The steps of a tick, in this order. A kit orders its own steps inside `BeforeCollision` and
/// `AfterCollision`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimSet {
    Inputs,
    BeforeCollision,
    Collision,
    AfterCollision,
    Mode,
    Visibility,
}

impl SimUpdate {
    /// The schedule with no game systems yet. The tick's random sequences start before
    /// `SimSet::Inputs`; after `SimSet::Visibility` the tick advances and its inputs are cleared. Two systems with
    /// conflicting access and no order fail the build, since either order could win.
    pub fn schedule() -> Schedule {
        let mut schedule = Schedule::new(SimUpdate);
        #[expect(
            clippy::disallowed_methods,
            reason = "the one place that sets the sim schedule's build settings"
        )]
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            ..ScheduleBuildSettings::new()
        });
        schedule
            .configure_sets(
                (
                    SimSet::Inputs,
                    SimSet::BeforeCollision,
                    SimSet::Collision,
                    SimSet::AfterCollision,
                    SimSet::Mode,
                    SimSet::Visibility,
                )
                    .chain(),
            )
            .add_systems((
                start_tick.before(SimSet::Inputs),
                end_tick.after(SimSet::Visibility),
            ));
        schedule
    }

    /// Inserts what the schedule needs into a new world: the tick at 0, the random sequences of
    /// `seed`, the id allocator, the entity index and empty tick inputs. A restore then replaces
    /// the state among them.
    pub fn prepare(world: &mut World, seed: SegmentSeed) {
        world.insert_resource(SimTick::default());
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

#[cfg(test)]
mod tests;
