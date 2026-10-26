#![expect(
    unsafe_code,
    reason = "Bevy's `BundleWriter` spawns a kind's parts by component id, an unsafe call (02-engine-core.md, Unsafe only where an outside API asks)"
)]

use std::alloc::Layout;
use std::{fmt, mem};

use bevy_ecs::bundle::{Bundle, BundleScratch, BundleWriter, NoBundleEffect};
use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::entity::Entity;
use bevy_ecs::ptr::{OwningPtr, move_as_ptr};
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{EntityWorldMut, World};
use bevy_platform::cell::SyncCell;

/// The scratch in which the parts of a spawning entity are made, then written onto it at once:
/// one move between archetypes for every part, where Bevy inserts a bundle only of types fixed at
/// compile time. A spawn takes the scratch through the world's exclusive access, so its cell is
/// `Sync` with no lock, as Bevy's system parameters hold their scratch: a resource of the scratch,
/// whose allocator is not `Sync`. Not state.
#[derive(Resource)]
pub struct SpawnParts(SyncCell<SpawnScratch>);

impl Default for SpawnParts {
    fn default() -> Self {
        Self(SyncCell::new(SpawnScratch::default()))
    }
}

/// What a spawn writes its parts through: the scratch of their values, and the components it
/// holds.
#[derive(Default)]
struct SpawnScratch {
    bundle: BundleScratch,
    held: Vec<ComponentId>,
}

/// The parts a spawning entity takes in its one write, as they are made: the world it is of, the
/// entity, the writer that holds them, and the components they are of.
pub struct PartWriter<'w, 's> {
    world: &'w World,
    entity: Entity,
    writer: BundleWriter<'s>,
    held: &'s mut Vec<ComponentId>,
}

impl SpawnParts {
    /// Gives `entity`, which spawns, the parts `push` makes from the world and the entity, in
    /// one move.
    pub fn give(entity: &mut EntityWorldMut<'_>, push: impl FnOnce(&mut PartWriter<'_, '_>)) {
        let held = entity.world_scope(Self::take);
        let mut scratch = held.unwrap_or_default();
        scratch.held.clear();
        let mut parts = PartWriter {
            world: entity.world(),
            entity: entity.id(),
            writer: scratch.bundle.writer(),
            held: &mut scratch.held,
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
                *parts.0.get() = scratch;
            }
        });
    }

    /// The match's scratch, which it lends a spawn; none in a world that holds none.
    fn take(world: &mut World) -> Option<SpawnScratch> {
        let mut parts = world.get_resource_mut::<Self>()?;
        Some(mem::take(parts.0.get()))
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
        self.push_by_id(id, part);
    }

    /// Adds `part` to the write, its component `id` of the world, as a declared part knows it.
    pub fn push_by_id<C: Component>(&mut self, id: ComponentId, part: C) {
        debug_assert_eq!(self.world.components().component_id::<C>(), Some(id));
        debug_assert!(!self.held.contains(&id), "a spawn takes each part once");
        self.held.push(id);
        OwningPtr::make(part, |part| {
            // SAFETY: `part` holds a `C`, of the component `id` of the world whose entity the
            // writer writes.
            unsafe {
                self.writer
                    .push_component_by_id(id, part, Layout::new::<C>());
            }
        });
    }

    /// Adds each part of `parts` to the write, as the caller of a spawn gives them; the spawn
    /// registered each of its components in the world first.
    pub fn push_bundle<B: Bundle<Effect: NoBundleEffect>>(&mut self, parts: B) {
        let components = self.world.components();
        let mut ids = B::get_component_ids(components);
        let (writer, held) = (&mut self.writer, &mut *self.held);
        move_as_ptr!(parts);
        // SAFETY: `get_components` gives each component of the bundle once, in the order of
        // `get_component_ids`, each a value of the component whose id and layout it pushes, of
        // the world whose entity the writer writes; the bundle has no effect to apply after.
        unsafe {
            B::get_components(parts, &mut |_, part| {
                let id = ids
                    .next()
                    .flatten()
                    .expect("a spawn's parts are of registered components");
                debug_assert!(!held.contains(&id), "a spawn takes each part once");
                held.push(id);
                let layout = components
                    .get_info(id)
                    .expect("a registered component has its info")
                    .layout();
                writer.push_component_by_id(id, part, layout);
            });
        }
    }

    /// Whether the write holds a part of the component `id`, as the spawn's caller gave it.
    pub fn holds(&self, id: ComponentId) -> bool {
        self.held.contains(&id)
    }
}

impl fmt::Debug for SpawnScratch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpawnScratch")
            .field("held", &self.held)
            .finish_non_exhaustive()
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
