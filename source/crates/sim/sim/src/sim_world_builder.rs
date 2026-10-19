use std::mem::ManuallyDrop;
use std::{fmt, thread};

use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::schedule::{Schedule, Schedules};
use bevy_ecs::world::World;
use campfire_common::SegmentSeed;

use crate::id_allocator::IdAllocator;
use crate::own_resources::OwnResources;
use crate::sim_rng::SimRng;
use crate::sim_tick::SimTick;
use crate::sim_update::SimUpdate;
use crate::stable_id::entity_index::EntityIndex;
use crate::stable_id::state_changes::StateChanges;
use crate::state_registry::StateRegistry;
use crate::state_registry::builder::StateRegistryBuilder;
use crate::state_registry::copy_queries::CopyQueries;
use crate::tick_inputs::TickInputs;
use crate::tick_rate::TickRate;

/// A sim's world as it is built: the sim's resources in place, and the tick's schedule and the
/// state registry that the installs fill. `build` puts both in the world, which then holds the
/// whole match; a builder dropped before it panics, as its world would hold neither. Every sim
/// world is built through one, a match's through the capabilities' match builder, so none lacks a
/// part of the sim.
#[must_use = "a match's build ends with `build`, which puts its schedule in its world"]
pub struct SimWorldBuilder<'w> {
    world: &'w mut World,
    schedule: Schedule,
    registry: StateRegistryBuilder,
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
    pub registry: &'a mut StateRegistryBuilder,
}

impl SimWorldBuilder<'_> {
    /// Visits, with `v`, the resources a match holds of its own, whatever its modules install:
    /// the sim's, those a server's state registry puts in as it tracks the state's changes, and
    /// Bevy's world and schedules.
    pub fn own_resources<V: OwnResources + ?Sized>(v: &mut V) {
        v.own::<SimTick>();
        v.own::<TickRate>();
        v.own::<SimRng>();
        v.own::<IdAllocator>();
        v.own::<EntityIndex>();
        v.own::<TickInputs>();
        v.own::<StateRegistry>();
        v.own::<StateChanges>();
        v.own::<CopyQueries>();
        v.own::<Schedules>();
        v.own::<DefaultQueryFilters>();
    }
}

impl<'w> SimWorldBuilder<'w> {
    /// Starts the build of a sim in `world`, a new world, at `rate`, drawing from `seed`.
    pub fn new(world: &'w mut World, seed: SegmentSeed, rate: TickRate) -> Self {
        SimUpdate::prepare(world, seed, rate);
        SimWorldBuilder {
            world,
            schedule: SimUpdate::schedule(),
            registry: StateRegistry::builder(),
            unbuilt: Unbuilt,
        }
    }

    pub const fn world(&mut self) -> &mut World {
        self.world
    }

    /// The world, the schedule and the registry at once, for an install that fills all three.
    pub const fn parts(&mut self) -> SimParts<'_> {
        SimParts {
            world: self.world,
            schedule: &mut self.schedule,
            registry: &mut self.registry,
        }
    }

    /// Builds the tick's schedule, so an error in its order shows here and not at the first
    /// tick, naming its systems, and puts it and the state registry in the world.
    pub fn build(self) {
        let SimWorldBuilder {
            world,
            mut schedule,
            registry,
            unbuilt,
        } = self;
        unbuilt.defuse();
        if let Err(error) = schedule.initialize(world) {
            let problem = error.to_string(schedule.graph(), world);
            panic!("the installs order the tick's systems: {problem}");
        }
        world.add_schedule(schedule);
        world.insert_resource(registry.build());
    }
}

impl Unbuilt {
    /// Ends the build: the mark goes with no panic.
    const fn defuse(self) {
        let _ = ManuallyDrop::new(self);
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::fmt;

    use bevy_ecs::resource::Resource;
    use bevy_ecs::schedule::{Schedule, Schedules};
    use bevy_ecs::world::World;
    use campfire_schedule::StrictSchedule;

    use crate::sim_update::SimUpdate;
    use crate::sim_world_builder::{SimWorldBuilder, Unbuilt};
    use crate::stable_id::state_changes::StateChanges;
    use crate::state_registry::StateRegistry;
    use crate::state_registry::builder::StateRegistryBuilder;

    /// The build of a test's match left open after its declarations: its tick's schedule, which
    /// holds none of its systems yet, and the end of its build, which installs those its books
    /// call for once the test loaded them.
    #[derive(Resource)]
    pub(crate) struct OpenBuild {
        schedule: Schedule,
        finish: for<'a> fn(SimWorldBuilder<'a>),
    }

    impl OpenBuild {
        pub(crate) const fn schedule_mut(&mut self) -> &mut Schedule {
            &mut self.schedule
        }
    }

    impl fmt::Debug for OpenBuild {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("OpenBuild")
                .field("schedule", &self.schedule.label())
                .finish_non_exhaustive()
        }
    }

    impl<'w> SimWorldBuilder<'w> {
        /// Ends the build with the tick's schedule left open, for a test that loads its books and
        /// types after its declarations: `finish`, which ends with `build`, ends it as the first
        /// tick begins, or as `finish_open` asks. The world holds the state registry meanwhile,
        /// and no tick schedule, so nothing runs a tick of no system.
        pub fn leave_open(self, finish: for<'a> fn(SimWorldBuilder<'a>)) {
            let SimWorldBuilder {
                world,
                schedule,
                registry,
                unbuilt,
            } = self;
            unbuilt.defuse();
            world.insert_resource(registry.build());
            world.insert_resource(OpenBuild { schedule, finish });
        }

        /// Ends the build of `world` as its `finish` does, when `leave_open` left it open.
        pub fn finish_open(world: &'w mut World) {
            let Some(OpenBuild { schedule, finish }) = world.remove_resource::<OpenBuild>() else {
                return;
            };
            let registry = SimWorldBuilder::registry_of(world);
            finish(SimWorldBuilder {
                world,
                schedule,
                registry,
                unbuilt: Unbuilt,
            });
        }

        /// The state types of `world`'s registry, which it gives up, to register more unless a
        /// record of its changes holds them.
        fn registry_of(world: &mut World) -> StateRegistryBuilder {
            let recorded = world.contains_resource::<StateChanges>();
            let registry = world.remove_resource::<StateRegistry>();
            let registry = registry.expect("a sim world holds its state registry");
            StateRegistryBuilder::reopened(registry, recorded)
        }

        /// Whether the build of `world` is open, as `leave_open` leaves it.
        pub fn is_open(world: &World) -> bool {
            world.contains_resource::<OpenBuild>()
        }

        /// Opens `world`, a sim world built before or left open, again, for an install after the
        /// build, as a test's mode installs into a match whose books the test loaded piece by
        /// piece.
        pub fn reopen(world: &'w mut World) -> Self {
            let schedule = match world.remove_resource::<OpenBuild>() {
                Some(open) => open.schedule,
                None => world
                    .resource_mut::<Schedules>()
                    .remove(SimUpdate)
                    .expect("a built match holds the tick's schedule"),
            };
            let registry = SimWorldBuilder::registry_of(world);
            SimWorldBuilder {
                world,
                schedule,
                registry,
                unbuilt: Unbuilt,
            }
        }

        /// Leaves Bevy's automatic sync points out of the tick's schedule: one orders the systems
        /// it lies between, so `build` then panics on every pair of systems that only a sync
        /// point keeps in order. A schedule keeps it through a reopen.
        pub fn without_sync_points(&mut self) {
            StrictSchedule::without_sync_points(&mut self.schedule);
        }

        /// Ends the build with the tick's schedule unbuilt, each system in its graph, for a test
        /// that reads what each accesses: the world then runs no tick.
        pub fn leave_unbuilt(self) {
            let SimWorldBuilder {
                world,
                schedule,
                registry,
                unbuilt,
            } = self;
            unbuilt.defuse();
            world.add_schedule(schedule);
            world.insert_resource(registry.build());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::num::NonZeroU32;
    use std::panic;

    use bevy_ecs::resource::Resource;

    use super::*;
    use crate::sim_world::SimWorld;
    use crate::state_registry::state_delta::StateDelta;

    /// The resources a visit of a build's own lists, by their ids.
    #[derive(Debug, Default)]
    struct OwnIds(Vec<TypeId>);

    impl OwnResources for OwnIds {
        fn own<R: Resource>(&mut self) {
            self.0.push(TypeId::of::<R>());
        }
    }

    const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

    /// What the end of an open build puts in, before its first tick runs.
    #[derive(Resource, Debug)]
    struct Finished;

    /// The end of an open build, which marks its world.
    fn finish(mut builder: SimWorldBuilder<'_>) {
        builder.world().insert_resource(Finished);
        builder.build();
    }

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
        let mut builder = SimWorldBuilder::new(&mut world, SegmentSeed::ZERO, RATE);
        assert!(!held(builder.world()));
        builder.build();
        assert!(held(&world));
        world.run_schedule(SimUpdate);
        assert_eq!(world.resource::<SimTick>().start().get(), 1);
        let mut builder = SimWorldBuilder::reopen(&mut world);
        assert!(!held(builder.world()));
        builder.build();
        assert!(held(&world));

        // A match of no module, its changes tracked, holds the sim's own resources alone.
        world
            .resource::<StateRegistry>()
            .clone()
            .track(&mut world, &mut StateDelta::default());
        let mut held: Vec<TypeId> = world
            .iter_resources()
            .filter_map(|(info, _)| info.type_id())
            .collect();
        let mut own = OwnIds::default();
        SimWorldBuilder::own_resources(&mut own);
        held.sort_unstable();
        own.0.sort_unstable();
        assert_eq!(held, own.0);

        // A build left open holds the registry and no tick schedule, until its first tick ends
        // it as its `finish` says, before the tick runs.
        let mut open = World::new();
        SimWorldBuilder::new(&mut open, SegmentSeed::ZERO, RATE).leave_open(finish);
        assert!(SimWorldBuilder::is_open(&open));
        let ticks = |world: &World| {
            let schedules = world.get_resource::<Schedules>();
            schedules.is_some_and(|schedules| schedules.contains(SimUpdate))
        };
        assert!(!ticks(&open) && open.contains_resource::<StateRegistry>());
        open.step();
        assert!(!SimWorldBuilder::is_open(&open) && open.contains_resource::<Finished>());
        assert!(ticks(&open));
        assert_eq!(open.resource::<SimTick>().start().get(), 1);

        // A builder dropped before `build` would leave its world with no schedule: it panics.
        let dropped = panic::catch_unwind(|| {
            let mut world = World::new();
            drop(SimWorldBuilder::new(&mut world, SegmentSeed::ZERO, RATE));
        });
        assert_eq!(
            dropped.unwrap_err().downcast_ref::<&str>(),
            Some(&"a match's build ends with `build`, which puts its schedule in its world")
        );
    }
}
