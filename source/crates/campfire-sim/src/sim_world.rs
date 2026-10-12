use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::world::{EntityRef, Mut, World};
use campfire_common::{SegmentSeed, StateHash};

use crate::entity_index::EntityIndex;
use crate::sim_update::SimUpdate;
use crate::sim_world_builder::SimWorldBuilder;
use crate::stable_id::StableId;
use crate::state_copy::StateCopy;
use crate::state_registry::breach::Breach;
use crate::state_registry::error::SnapshotError;
use crate::state_registry::state_delta::StateDelta;
use crate::state_registry::{StateRegistry, TypeHash};
use crate::tick_rate::TickRate;
use crate::unpredicted::Unpredicted;

/// What the sim gives a world it built: its entities by stable id, its ticks, and its state's
/// hash, snapshot and restore, through the `StateRegistry` it holds.
pub trait SimWorld {
    /// The entity of `id`, while it lives.
    fn entity_of(&self, id: StableId) -> Option<Entity>;

    /// The entity of `id` with its components, while it lives.
    fn entity_ref_of(&self, id: StableId) -> Option<EntityRef<'_>>;

    /// Runs one sim tick in a world no `App` updates, so it clears the change trackers after the
    /// tick, as an `App` does after each frame.
    fn step(&mut self);

    /// The hash of the state.
    fn state_hash(&self) -> StateHash;

    /// Writes a snapshot of the state into `out`, and gives its hash.
    fn snapshot(&self, out: &mut Vec<u8>) -> StateHash;

    /// Restores `snapshot` into the world, which holds no sim entities, as
    /// `StateRegistry::restore` does.
    fn restore(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError>;

    /// The hash of the state, and in `per_type` each state type's own.
    fn state_hash_by_type(&self, per_type: &mut Vec<TypeHash>) -> StateHash;

    /// Starts recording the changes of the state, and writes the whole state into `delta`, as
    /// `StateRegistry::track` does.
    fn track(&mut self, delta: &mut StateDelta);

    /// Writes into `delta` the state that changed since the last copy, as
    /// `StateRegistry::changes` does.
    fn changes(&mut self, delta: &mut StateDelta);

    /// A copy of the state, which from then on records its changes for the copy to follow.
    fn state_copy(&mut self) -> StateCopy;

    /// The first entity, by stable id, that `breaks` says breaks a rule a restored state keeps;
    /// none when none does.
    fn first_breach(&self, breaks: impl Fn(Entity) -> bool) -> Result<(), Breach>;

    /// Hides every unit with the `Unpredicted` marker from the queries that do not name it.
    fn register_unpredicted(&mut self);

    /// Starts the build of a sim in the world, a new one, at `rate`, drawing from `seed`.
    fn sim_builder(&mut self, seed: SegmentSeed, rate: TickRate) -> SimWorldBuilder<'_>;

    /// Opens the world, a sim world built before, again, for an install after the build, as a
    /// mode installs into a match whose capabilities are in place.
    fn reopen_sim(&mut self) -> SimWorldBuilder<'_>;
}

impl SimWorld for World {
    fn entity_of(&self, id: StableId) -> Option<Entity> {
        self.resource::<EntityIndex>().get(id)
    }

    fn entity_ref_of(&self, id: StableId) -> Option<EntityRef<'_>> {
        Some(self.entity(self.entity_of(id)?))
    }

    fn step(&mut self) {
        self.run_schedule(SimUpdate);
        self.clear_trackers();
    }

    fn state_hash(&self) -> StateHash {
        self.resource::<StateRegistry>().hash(self)
    }

    fn snapshot(&self, out: &mut Vec<u8>) -> StateHash {
        self.resource::<StateRegistry>().snapshot(self, out)
    }

    fn restore(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
        self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
            registry.restore(snapshot, world)
        })
    }

    fn state_hash_by_type(&self, per_type: &mut Vec<TypeHash>) -> StateHash {
        self.resource::<StateRegistry>()
            .hash_by_type(self, per_type)
    }

    fn track(&mut self, delta: &mut StateDelta) {
        self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
            registry.track(world, delta);
        });
    }

    fn changes(&mut self, delta: &mut StateDelta) {
        self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
            registry.changes(world, delta);
        });
    }

    fn state_copy(&mut self) -> StateCopy {
        let mut delta = StateDelta::default();
        let mut copy = World::new();
        copy.init_resource::<EntityIndex>();
        self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
            registry.track(world, &mut delta);
            registry.apply(&delta, &mut copy);
        });
        StateCopy::of(copy, delta)
    }

    fn first_breach(&self, breaks: impl Fn(Entity) -> bool) -> Result<(), Breach> {
        let index = self.resource::<EntityIndex>();
        match index.iter().find(|&(_, entity)| breaks(entity)) {
            Some((_, entity)) => Err(Breach::Entity(entity)),
            None => Ok(()),
        }
    }

    fn register_unpredicted(&mut self) {
        let id = self.register_component::<Unpredicted>();
        self.resource_mut::<DefaultQueryFilters>()
            .register_disabling_component(id);
    }

    fn sim_builder(&mut self, seed: SegmentSeed, rate: TickRate) -> SimWorldBuilder<'_> {
        SimWorldBuilder::new(self, seed, rate)
    }

    fn reopen_sim(&mut self) -> SimWorldBuilder<'_> {
        SimWorldBuilder::reopen(self)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::collections::BTreeSet;

    use bevy_ecs::schedule::{IntoScheduleConfigs, IntoSystemSet, SystemSet};
    use bevy_ecs::system::ResMut;
    use bevy_ecs::world::{Mut, World};
    use campfire_schedule::StrictSchedule;

    use crate::sim_update::stage_clock::StageClock;
    use crate::sim_update::{SimEdge, SimSet, SimUpdate, end_tick, start_tick};
    use crate::state_registry::StateRegistry;
    use crate::state_registry::internals::{Draws, Mutation};
    use crate::tick_inputs::{TickInput, TickInputs};
    use campfire_common::PlayerSlot;

    /// What the sim gives a world it built for the tests and benches of every crate: its
    /// schedule's checks, a clock of its stages, and changes to its state no input makes.
    pub trait SimWorldInternals {
        /// Builds the sim schedule again with no automatic sync points, and the build's other
        /// settings kept: one of them orders the systems it lies between, so the ambiguity check
        /// then sees every pair of systems that only a sync point keeps in order. The error names
        /// its systems.
        fn build_without_sync_points(&mut self) -> Result<(), String>;

        /// The name of each system of the sim schedule that sits in no stage and no `SimEdge`,
        /// but the tick's own start and end. Such a system runs in a gap between two stages, in
        /// an order Bevy picks, so a bench's probes time it with either stage. It builds the
        /// schedule as `build_without_sync_points` does, so a sync point it finds is one the
        /// code added.
        fn systems_outside_stages(&mut self) -> Vec<String>;

        /// Adds a probe at each edge of the sim schedule, after the stage before it and its
        /// `SimEdge::After` and before the stage after it, and the `StageClock` they write.
        fn install_stage_clock(&mut self);

        /// The name of each state type the match registers, in their order.
        fn state_names(&self) -> Vec<&'static str>;

        /// Puts a drawn value of the state type `name` in place of one holder's; false when
        /// nothing holds one, or no draw decodes.
        fn scramble_state(&mut self, name: &str, draws: &mut Draws) -> bool;

        /// Changes where the state type `name` sits, as `mutation` says; false when the type has
        /// nothing to change so.
        fn mutate_state(&mut self, name: &str, draws: &mut Draws, mutation: Mutation) -> bool;

        /// Queues player `slot`'s input of `payload` for the next tick, as a session's log gives
        /// one.
        fn push_input(&mut self, slot: PlayerSlot, payload: &[u8]);
    }

    impl SimWorldInternals for World {
        fn build_without_sync_points(&mut self) -> Result<(), String> {
            self.schedule_scope(SimUpdate, |world, schedule| {
                StrictSchedule::without_sync_points(schedule);
                let built = schedule.initialize(world).map(drop);
                built.map_err(|error| error.to_string(schedule.graph(), world))
            })
        }

        fn systems_outside_stages(&mut self) -> Vec<String> {
            if let Err(error) = self.build_without_sync_points() {
                panic!("{error}");
            }
            self.schedule_scope(SimUpdate, |_, schedule| {
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

        fn install_stage_clock(&mut self) {
            self.insert_resource(StageClock::started());
            let probe = |edge: usize| move |mut clock: ResMut<'_, StageClock>| clock.pass(edge);
            self.schedule_scope(SimUpdate, |_, schedule| {
                schedule.add_systems(probe(0).after(SimEdge::Start).before(SimSet::Inputs));
                for (edge, stages) in SimSet::ALL.windows(2).enumerate() {
                    schedule.add_systems(
                        probe(edge + 1)
                            .after(SimEdge::After(stages[0]))
                            .before(stages[1]),
                    );
                }
                schedule.add_systems(
                    probe(StageClock::EDGES - 1)
                        .after(SimEdge::After(SimSet::Vision))
                        .before(end_tick),
                );
            });
        }

        fn state_names(&self) -> Vec<&'static str> {
            self.resource::<StateRegistry>().names().collect()
        }

        fn scramble_state(&mut self, name: &str, draws: &mut Draws) -> bool {
            self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
                registry.scramble(world, name, draws)
            })
        }

        fn mutate_state(&mut self, name: &str, draws: &mut Draws, mutation: Mutation) -> bool {
            self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
                registry.mutate(world, name, draws, mutation)
            })
        }

        fn push_input(&mut self, slot: PlayerSlot, payload: &[u8]) {
            self.resource_mut::<TickInputs>()
                .push(TickInput { slot, payload });
        }
    }
}
