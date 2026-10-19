//! Deferred structural entity commands.
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{SystemBuffer, SystemMeta};
use bevy_ecs::world::{DeferredWorld, World};
use bevy_platform::cell::SyncCell;
use bevy_platform::collections::HashMap;
use bevy_replicon::shared::replication::deferred_entity::{DeferredEntity, EntityScratch};
use core::any::{Any, TypeId};

/// Batched structural changes for multiple entities, kept in the system state as
/// [`Commands`](bevy_ecs::system::Commands) are.
///
/// A system takes it as a [`Deferred`](bevy_ecs::system::Deferred) parameter. Each change is an
/// op in queue order, and each component a value in its type's own buffer, so queuing allocates
/// nothing once the buffers have grown. When the system's buffers apply, the changes of each entity
/// flush through one Replicon [`DeferredEntity`]: its removals as one bundle, then its insertions
/// as one bundle, so an entity takes at most one insertion of a component in each apply.
#[derive(Default)]
pub struct DeferredEntityCommands {
    ops: Vec<DeferredOp>,
    /// The queued values of each component type, by the index its ops name.
    stores: Vec<Box<dyn ErasedValues>>,
    /// The index of each component type's store.
    store_of: HashMap<TypeId, usize>,
    scratch: EntityScratch,
}

#[derive(Debug, Clone, Copy)]
struct DeferredOp {
    entity: Entity,
    /// The op's place in queue order, which keeps the order of one entity's ops deterministic.
    sequence: usize,
    /// The store of the op's component type.
    store: usize,
    change: Change,
}

/// What an op does with its component: insert the value at its index in the store, or remove the
/// component.
#[derive(Debug, Clone, Copy)]
enum Change {
    Insert(usize),
    Remove,
}

/// The queued values of one component type.
#[derive(Debug)]
struct Values<C> {
    values: Vec<Option<C>>,
}

/// Where [`DeferredEntityCommands`] keeps a queued value: its type's store, and its index there.
#[derive(Debug, Clone, Copy)]
struct StoredValue {
    store: usize,
    index: usize,
}

/// [`Values`] of any component type.
trait ErasedValues: Send + Sync {
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Applies `change` to `entity`, taking its value out of the store.
    fn apply(&mut self, change: Change, entity: &mut DeferredEntity<'_>);

    /// Drops every value left, of ops whose entity was gone.
    fn clear(&mut self);

    #[cfg(test)]
    fn capacity(&self) -> usize;
}

impl core::fmt::Debug for DeferredEntityCommands {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeferredEntityCommands")
            .field("ops", &self.ops.len())
            .field("component_types", &self.stores.len())
            .finish_non_exhaustive()
    }
}

impl DeferredEntityCommands {
    /// Queues insertion of one component on `entity`.
    pub fn insert<C: Component>(&mut self, entity: Entity, component: C) {
        let StoredValue { store, index } = self.write(component);
        self.push(entity, store, Change::Insert(index));
    }

    /// Queues removal of one component from `entity`.
    pub fn remove<C: Component>(&mut self, entity: Entity) {
        let store = self.store::<C>();
        self.push(entity, store, Change::Remove);
    }

    /// The index of `C`'s store, which goes in at the first op of `C`.
    fn store<C: Component>(&mut self) -> usize {
        *self.store_of.entry(TypeId::of::<C>()).or_insert_with(|| {
            self.stores
                .push(Box::new(Values::<C> { values: Vec::new() }));
            self.stores.len() - 1
        })
    }

    /// The store at `store`, which is `C`'s.
    fn typed<C: Component>(&mut self, store: usize) -> &mut Values<C> {
        self.stores[store]
            .as_any_mut()
            .downcast_mut()
            .expect("a component type's store holds its values")
    }

    /// Adds `component` to its type's store.
    fn write<C: Component>(&mut self, component: C) -> StoredValue {
        let store = self.store::<C>();
        let values = &mut self.typed::<C>(store).values;
        values.push(Some(component));
        StoredValue {
            store,
            index: values.len() - 1,
        }
    }

    fn push(&mut self, entity: Entity, store: usize, change: Change) {
        self.ops.push(DeferredOp {
            entity,
            sequence: self.ops.len(),
            store,
            change,
        });
    }

    /// Applies every queued change, those of each entity in one flush. The changes of a
    /// despawned entity are dropped.
    pub fn apply_to(&mut self, world: &mut World) {
        let mut ops = core::mem::take(&mut self.ops);
        ops.sort_unstable_by_key(|op| (op.entity, op.sequence));
        for group in ops.chunk_by(|a, b| a.entity == b.entity) {
            let Ok(entity_mut) = world.get_entity_mut(group[0].entity) else {
                continue;
            };
            let mut deferred = DeferredEntity::new(entity_mut, &mut self.scratch);
            for op in group {
                self.stores[op.store].apply(op.change, &mut deferred);
            }
            deferred.flush();
        }
        for store in &mut self.stores {
            store.clear();
        }
        ops.clear();
        self.ops = ops;
    }
}

impl<C: Component> ErasedValues for Values<C> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn apply(&mut self, change: Change, entity: &mut DeferredEntity<'_>) {
        let mut take = |index: usize| {
            self.values[index]
                .take()
                .expect("each queued value is applied once")
        };
        match change {
            Change::Insert(index) => {
                entity.insert(take(index));
            }
            Change::Remove => {
                entity.remove::<C>();
            }
        }
    }

    fn clear(&mut self) {
        self.values.clear();
    }

    #[cfg(test)]
    fn capacity(&self) -> usize {
        self.values.capacity()
    }
}

/// The applied buffers of observers, empty with their capacity kept, which the next observer run
/// that queues changes takes in place of its own. Each observer run queues its changes as one
/// command that holds them, so without these every run would grow its buffers again. It keeps as
/// many as the most runs that queued before one flush, as Bevy's command queue keeps its peak
/// capacity. The cell makes the resource `Sync`, as Replicon's scratch is not.
#[derive(Resource)]
struct SpareBuffers(SyncCell<Vec<DeferredEntityCommands>>);

impl Default for SpareBuffers {
    fn default() -> Self {
        Self(SyncCell::new(Vec::new()))
    }
}

impl core::fmt::Debug for SpareBuffers {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SpareBuffers").finish_non_exhaustive()
    }
}

impl SystemBuffer for DeferredEntityCommands {
    fn apply(&mut self, _system_meta: &SystemMeta, world: &mut World) {
        self.apply_to(world);
    }

    /// An observer queues its changes as one command, which takes the buffers with it and leaves
    /// a spare in their place, and gives them back as a spare once it applies.
    fn queue(&mut self, _system_meta: &SystemMeta, mut world: DeferredWorld<'_>) {
        if self.ops.is_empty() {
            return;
        }
        let spare = world
            .get_resource_mut::<SpareBuffers>()
            .and_then(|mut spares| spares.0.get().pop())
            .unwrap_or_default();
        let mut taken = core::mem::replace(self, spare);
        world.commands().queue(move |world: &mut World| {
            taken.apply_to(world);
            world
                .get_resource_or_init::<SpareBuffers>()
                .0
                .get()
                .push(taken);
        });
    }
}

#[cfg(test)]
mod tests;
