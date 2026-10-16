use std::alloc::Layout;
use std::sync::Mutex;
use std::{fmt, mem};

use bevy_ecs::bundle::{BundleScratch, BundleWriter};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::ptr::OwningPtr;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{EntityWorldMut, World};

/// The scratch in which the parts of a spawning entity are made, then written onto it at once:
/// one move between archetypes for every part, where Bevy inserts a bundle only of types fixed at
/// compile time. Its mutex is never locked, as a spawn takes the scratch through the world's
/// exclusive access: it makes a resource of the scratch, whose allocator is not `Sync`. Not
/// state.
#[derive(Resource, Default)]
pub struct SpawnParts(Mutex<BundleScratch>);

/// The parts a spawning entity takes in its one write, as they are made: the world it is of, the
/// entity, and the writer that holds them.
pub struct PartWriter<'w, 's> {
    world: &'w World,
    entity: Entity,
    writer: BundleWriter<'s>,
}

impl SpawnParts {
    /// Gives `entity`, which spawns, the parts `push` makes from the world and the entity, in
    /// one move.
    pub fn give(entity: &mut EntityWorldMut<'_>, push: impl FnOnce(&mut PartWriter<'_, '_>)) {
        let held = entity.world_scope(Self::take);
        let mut scratch = held.unwrap_or_default();
        let mut parts = PartWriter {
            world: entity.world(),
            entity: entity.id(),
            writer: scratch.writer(),
        };
        push(&mut parts);
        let writer = parts.writer;
        if !writer.is_empty() {
            // SAFETY: each part `push` pushed is of a component of the world `entity` is of, as
            // `PartWriter::push` finds its id in that world.
            unsafe { writer.write(entity) };
        }
        entity.world_scope(|world| {
            if let Some(mut parts) = world.get_resource_mut::<Self>() {
                *parts.0.get_mut().expect("no spawn panicked holding it") = scratch;
            }
        });
    }

    /// The match's scratch, which it lends a spawn; none in a world that holds none.
    fn take(world: &mut World) -> Option<BundleScratch> {
        let mut parts = world.get_resource_mut::<Self>()?;
        let scratch = parts.0.get_mut().expect("no spawn panicked holding it");
        Some(mem::take(scratch))
    }
}

impl<'w> PartWriter<'w, '_> {
    pub const fn world(&self) -> &'w World {
        self.world
    }

    pub const fn entity(&self) -> Entity {
        self.entity
    }

    /// Adds `part` to the write; its component is one the modules declared, so registered in
    /// the world.
    pub fn push<C: Component>(&mut self, part: C) {
        let id = self
            .world
            .components()
            .component_id::<C>()
            .expect("a part's component is registered as its module declared it");
        OwningPtr::make(part, |part| {
            // SAFETY: `part` holds a `C`, of the component `id` of the world whose entity the
            // writer writes.
            unsafe {
                self.writer
                    .push_component_by_id(id, part, Layout::new::<C>());
            }
        });
    }
}

impl fmt::Debug for SpawnParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SpawnParts")
    }
}

impl fmt::Debug for PartWriter<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartWriter")
            .field("entity", &self.entity)
            .finish_non_exhaustive()
    }
}
