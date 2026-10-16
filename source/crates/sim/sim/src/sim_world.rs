use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::world::{EntityRef, Mut, World};
use campfire_common::StateHash;

use crate::sim_update::SimUpdate;
use crate::stable_id::StableId;
use crate::stable_id::entity_index::EntityIndex;
use crate::state_registry::error::SnapshotError;
use crate::state_registry::state_delta::StateDelta;
use crate::state_registry::{StateRegistry, TypeHash};

/// What the sim gives a world it built: its entities by stable id, its ticks, and its state's
/// hash, snapshot and restore, through the `StateRegistry` it holds.
pub trait SimWorld {
    /// The entity of `id`, while it lives.
    fn entity_of(&self, id: StableId) -> Option<Entity>;

    /// The entity of `id` with its components, while it lives.
    fn entity_ref_of(&self, id: StableId) -> Option<EntityRef<'_>>;

    /// The entity of `id`, while it lives and no disabling component hides it from queries, as
    /// `Unpredicted` hides a unit a predicting client holds as its authority sent it: the unit a
    /// query that names no disabling component finds, for a system that reaches units by id.
    fn queried_entity_of(&self, id: StableId) -> Option<Entity>;

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

    /// Despawns every sim entity, so the world takes a restore again, as a target that checks
    /// one snapshot after another does.
    fn despawn_state(&mut self);

    /// The hash of the state, and in `per_type` each state type's own.
    fn state_hash_by_type(&self, per_type: &mut Vec<TypeHash>) -> StateHash;

    /// Starts recording the changes of the state, and writes the whole state into `delta`, as
    /// `StateRegistry::track` does.
    fn track(&mut self, delta: &mut StateDelta);

    /// Writes into `delta` the state that changed since the last copy, as
    /// `StateRegistry::changes` does.
    fn changes(&mut self, delta: &mut StateDelta);
}

impl SimWorld for World {
    fn entity_of(&self, id: StableId) -> Option<Entity> {
        self.resource::<EntityIndex>().get(id)
    }

    fn entity_ref_of(&self, id: StableId) -> Option<EntityRef<'_>> {
        Some(self.entity(self.entity_of(id)?))
    }

    fn queried_entity_of(&self, id: StableId) -> Option<Entity> {
        let entity = self.entity_of(id)?;
        let unit = self.entity(entity);
        let filters = self.resource::<DefaultQueryFilters>();
        let hidden = filters
            .disabling_ids()
            .any(|component| unit.contains_id(component));
        (!hidden).then_some(entity)
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

    fn despawn_state(&mut self) {
        let entities: Vec<Entity> = self
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .collect();
        for entity in entities {
            self.despawn(entity);
        }
        debug_assert!(
            self.resource::<EntityIndex>().iter().next().is_none(),
            "a despawn unindexes its entity"
        );
    }

    fn state_hash_by_type(&self, per_type: &mut Vec<TypeHash>) -> StateHash {
        self.resource::<StateRegistry>()
            .hash_by_type(self, per_type)
    }

    fn track(&mut self, delta: &mut StateDelta) {
        self.resource_scope(|world, mut registry: Mut<'_, StateRegistry>| {
            registry.track(world, delta);
        });
    }

    fn changes(&mut self, delta: &mut StateDelta) {
        self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
            registry.changes(world, delta);
        });
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeSet;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::component::{Component, Mutable};
    use bevy_ecs::entity::Entity;
    use bevy_ecs::schedule::{InternedSystemSet, IntoSystemSet, SystemSet};
    use bevy_ecs::world::{Mut, World};
    use campfire_common::{PlayerSlot, Tick};
    use campfire_schedule::StrictSchedule;

    use crate::restoring::Restoring;
    use crate::sim_tick::SimTick;
    use crate::sim_update::{SimUpdate, end_tick, start_tick};
    use crate::sim_world::SimWorld;
    use crate::stable_id::StableId;
    use crate::stable_id::entity_index::EntityIndex;
    use crate::state_registry::StateRegistry;
    use crate::state_registry::error::SnapshotError;
    use crate::state_registry::internals::{Draws, Mutation};
    use crate::tick_inputs::{TickInput, TickInputs};

    /// What the sim gives a world it built for the tests and benches of every crate: its
    /// schedule's checks, changes to its state no input makes, its ticks with inputs, and its
    /// units' parts by stable id.
    pub trait SimWorldInternals {
        /// Builds the sim schedule again with no automatic sync points, and the build's other
        /// settings kept: one of them orders the systems it lies between, so the ambiguity check
        /// then sees every pair of systems that only a sync point keeps in order. The error names
        /// its systems.
        fn build_without_sync_points(&mut self) -> Result<(), String>;

        /// The name of each system of the sim schedule that sits in none of `sets`, but the
        /// tick's own start and end: with the modules' declared sets, a system no module set
        /// holds, which may run in a gap between two stages, in an order Bevy picks, so a
        /// bench's probes time it with either stage. It builds the schedule as
        /// `build_without_sync_points` does, so a sync point it finds is one the code added.
        fn systems_outside(&mut self, sets: &[InternedSystemSet]) -> Vec<String>;

        /// The name of each state type the match registers, in their order.
        fn state_names(&self) -> Vec<&'static str>;

        /// Puts a drawn value of the state type `name` in place of one holder's, or where none
        /// is; false when no draw decodes.
        fn scramble_state(&mut self, name: &str, draws: &mut Draws) -> bool;

        /// Changes where the state type `name` sits, as `mutation` says; false when the type has
        /// nothing to change so.
        fn mutate_state(&mut self, name: &str, draws: &mut Draws, mutation: Mutation) -> bool;

        /// Queues player `slot`'s input of `payload` for the next tick, as a session's log gives
        /// one.
        fn push_input(&mut self, slot: PlayerSlot, payload: &[u8]);

        /// Runs a tick in which each slot sends its payload, in their order.
        fn step_with(&mut self, inputs: &[(PlayerSlot, &[u8])]);

        /// The tick the match runs next.
        fn now(&self) -> Tick;

        /// Runs ticks until the next is `tick`.
        fn run_until(&mut self, tick: u64);

        /// Unit `id`'s part `C`; panics when the unit is gone or lacks it.
        fn part<C: Component>(&self, id: StableId) -> &C;

        /// Unit `id`'s part `C`; `None` when the unit is gone or lacks it.
        fn try_part<C: Component>(&self, id: StableId) -> Option<&C>;

        /// Unit `id`'s part `C`, to change; panics when the unit is gone or lacks it.
        fn part_mut<C: Component<Mutability = Mutable>>(&mut self, id: StableId) -> Mut<'_, C>;

        /// The entity of unit `id`; panics when the unit is gone.
        fn unit_entity(&self, id: StableId) -> Entity;

        /// Inserts `parts` on unit `id`.
        fn insert_parts(&mut self, id: StableId, parts: impl Bundle);

        /// Restores `snapshot` into a restore target, in place of what the last restore left.
        fn restore_in_place(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError>;
    }

    impl SimWorldInternals for World {
        fn build_without_sync_points(&mut self) -> Result<(), String> {
            self.schedule_scope(SimUpdate, |world, schedule| {
                StrictSchedule::without_sync_points(schedule);
                let built = schedule.initialize(world).map(drop);
                built.map_err(|error| error.to_string(schedule.graph(), world))
            })
        }

        fn systems_outside(&mut self, sets: &[InternedSystemSet]) -> Vec<String> {
            if let Err(error) = self.build_without_sync_points() {
                panic!("{error}");
            }
            self.schedule_scope(SimUpdate, |_, schedule| {
                let graph = schedule.graph();
                let sets = sets.iter().copied().chain([
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

        fn state_names(&self) -> Vec<&'static str> {
            self.resource::<StateRegistry>().names().collect()
        }

        fn scramble_state(&mut self, name: &str, draws: &mut Draws) -> bool {
            self.insert_resource(Restoring);
            let scrambled = self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
                registry.scramble(world, name, draws)
            });
            self.remove_resource::<Restoring>();
            scrambled
        }

        fn mutate_state(&mut self, name: &str, draws: &mut Draws, mutation: Mutation) -> bool {
            self.insert_resource(Restoring);
            let mutated = self.resource_scope(|world, registry: Mut<'_, StateRegistry>| {
                registry.mutate(world, name, draws, mutation)
            });
            self.remove_resource::<Restoring>();
            mutated
        }

        fn push_input(&mut self, slot: PlayerSlot, payload: &[u8]) {
            self.resource_mut::<TickInputs>()
                .push(TickInput { slot, payload });
        }

        fn step_with(&mut self, inputs: &[(PlayerSlot, &[u8])]) {
            for &(slot, payload) in inputs {
                self.push_input(slot, payload);
            }
            self.step();
        }

        fn now(&self) -> Tick {
            self.resource::<SimTick>().start()
        }

        fn run_until(&mut self, tick: u64) {
            while self.now().get() < tick {
                self.step();
            }
        }

        fn part<C: Component>(&self, id: StableId) -> &C {
            self.try_part(id).expect("a unit with the part")
        }

        fn try_part<C: Component>(&self, id: StableId) -> Option<&C> {
            self.get::<C>(self.entity_of(id)?)
        }

        fn part_mut<C: Component<Mutability = Mutable>>(&mut self, id: StableId) -> Mut<'_, C> {
            let entity = self.unit_entity(id);
            self.get_mut::<C>(entity).expect("a unit with the part")
        }

        fn unit_entity(&self, id: StableId) -> Entity {
            self.entity_of(id).expect("a unit of the match")
        }

        fn insert_parts(&mut self, id: StableId, parts: impl Bundle) {
            let entity = self.unit_entity(id);
            self.entity_mut(entity).insert(parts);
        }

        fn restore_in_place(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
            let entities: Vec<Entity> = self
                .resource::<EntityIndex>()
                .iter()
                .map(|(_, entity)| entity)
                .collect();
            for entity in entities {
                self.despawn(entity);
            }
            self.restore(snapshot)
        }
    }
}
