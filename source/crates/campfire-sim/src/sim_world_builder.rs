use std::{fmt, mem, thread};

use bevy_ecs::schedule::{Schedule, Schedules};
use bevy_ecs::world::World;
use campfire_common::SegmentSeed;

use crate::sim_update::SimUpdate;
use crate::state_registry::StateRegistry;
use crate::tick_rate::TickRate;

/// A sim's world as it is built: the sim's resources in place, and the tick's schedule and the
/// state registry that the installs fill. `build` puts both in the world, which then holds the
/// whole match; a builder dropped before it panics, as its world would hold neither. Every sim world is built
/// through one, a match's through the capabilities' match builder, so none lacks a part of the
/// sim.
#[must_use = "a match's build ends with `build`, which puts its schedule in its world"]
pub struct SimWorldBuilder<'w> {
    world: &'w mut World,
    schedule: Schedule,
    registry: StateRegistry,
    unbuilt: Unbuilt,
}

/// The mark of a build not ended: dropped, it panics, unless `build` defuses it. A field of its
/// own, so the builder has no `Drop` of its own, and `build` takes its parts by destructuring.
#[derive(Debug)]
struct Unbuilt;

/// The parts of a sim world being built, borrowed at once: what an install that fills the world,
/// the schedule and the registry together destructures, as the capabilities' match builder
/// installs the engine's modules and a mode. Test systems and components go in after `build`,
/// through the world's `Schedules` and `StateRegistry`.
pub struct SimParts<'a> {
    pub world: &'a mut World,
    pub schedule: &'a mut Schedule,
    pub registry: &'a mut StateRegistry,
}

impl<'w> SimWorldBuilder<'w> {
    /// Starts the build of a sim in `world`, a new world, at `rate`, drawing from `seed`.
    pub(crate) fn new(
        world: &'w mut World,
        seed: SegmentSeed,
        rate: TickRate,
    ) -> SimWorldBuilder<'w> {
        SimUpdate::prepare(world, seed, rate);
        SimWorldBuilder {
            world,
            schedule: SimUpdate::schedule(),
            registry: StateRegistry::new(),
            unbuilt: Unbuilt,
        }
    }

    /// Opens `world`, a sim world built before, again, for an install after the build, as a mode
    /// installs into a match whose capabilities are in place.
    pub(crate) fn reopen(world: &'w mut World) -> SimWorldBuilder<'w> {
        let schedule = world
            .resource_mut::<Schedules>()
            .remove(SimUpdate)
            .expect("a built match holds the tick's schedule");
        let registry = world
            .remove_resource::<StateRegistry>()
            .expect("a built match holds its state registry");
        SimWorldBuilder {
            world,
            schedule,
            registry,
            unbuilt: Unbuilt,
        }
    }

    pub const fn world(&mut self) -> &mut World {
        self.world
    }

    /// The world, the schedule and the registry at once, for an install that fills all three.
    pub fn parts(&mut self) -> SimParts<'_> {
        SimParts {
            world: self.world,
            schedule: &mut self.schedule,
            registry: &mut self.registry,
        }
    }

    /// Builds the tick's schedule, so an error in its order shows here and not at the first
    /// tick, and puts it and the state registry in the world.
    pub fn build(self) {
        self.finish()
            .unwrap_or_else(|problem| panic!("the installs order the tick's systems: {problem}"));
    }

    /// Builds the tick's schedule, then puts it and the registry in the world; the build's
    /// error, which names its systems, when the installs order the systems wrong.
    fn finish(self) -> Result<(), String> {
        let SimWorldBuilder {
            world,
            mut schedule,
            registry,
            unbuilt,
        } = self;
        unbuilt.defuse();
        let built = schedule.initialize(world).map(drop);
        let built = built.map_err(|error| error.to_string(schedule.graph(), world));
        world.add_schedule(schedule);
        world.insert_resource(registry);
        built
    }
}

impl Unbuilt {
    /// Ends the build: the mark goes with no panic.
    fn defuse(self) {
        mem::forget(self);
    }
}

impl Drop for Unbuilt {
    fn drop(&mut self) {
        assert!(
            thread::panicking(),
            "a match's build ends with `build`, which puts its schedule in its world"
        );
    }
}

impl fmt::Debug for SimWorldBuilder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimWorldBuilder")
            .field("schedule", &self.schedule.label())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for SimParts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimParts")
            .field("schedule", &self.schedule.label())
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use campfire_schedule::StrictSchedule;

    use crate::sim_world_builder::SimWorldBuilder;

    impl SimWorldBuilder<'_> {
        /// Leaves Bevy's automatic sync points out of the tick's schedule: one orders the systems
        /// it lies between, so `build` then panics on every pair of systems that only a sync
        /// point keeps in order. A schedule keeps it through a reopen.
        pub fn without_sync_points(&mut self) {
            StrictSchedule::without_sync_points(&mut self.schedule);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::panic;

    use super::*;
    use crate::sim_tick::SimTick;
    use crate::sim_world::SimWorld;

    const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

    #[test]
    fn a_build_puts_its_schedule_in_the_world_and_one_dropped_unbuilt_panics() {
        // During a build the world holds no tick schedule and no registry, as the builder fills
        // them; `build` puts them in, and the schedule runs a tick. A reopen takes them out
        // again, and `build` puts them back.
        let mut world = World::new();
        let held = |world: &World| {
            world
                .get_resource::<Schedules>()
                .is_some_and(|schedules| schedules.contains(SimUpdate))
                && world.contains_resource::<StateRegistry>()
        };
        let mut builder = world.sim_builder(SegmentSeed::new([0; 32]), RATE);
        assert!(!held(builder.world()));
        builder.build();
        assert!(held(&world));
        world.run_schedule(SimUpdate);
        assert_eq!(world.resource::<SimTick>().start().get(), 1);
        let mut builder = world.reopen_sim();
        assert!(!held(builder.world()));
        builder.build();
        assert!(held(&world));

        // A builder dropped before `build` would leave its world with no schedule: it panics.
        let dropped = panic::catch_unwind(|| {
            let mut world = World::new();
            drop(world.sim_builder(SegmentSeed::new([0; 32]), RATE));
        });
        assert_eq!(
            dropped.unwrap_err().downcast_ref::<&str>(),
            Some(&"a match's build ends with `build`, which puts its schedule in its world")
        );
    }
}
