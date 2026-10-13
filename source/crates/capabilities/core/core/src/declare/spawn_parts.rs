use std::sync::Mutex;
use std::{fmt, mem};

use bevy_ecs::bundle::{BundleScratch, BundleWriter};
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{EntityWorldMut, World};

/// The scratch in which the parts the modules declare for a spawning entity are made, then
/// written onto it at once: one move between archetypes for every part, where Bevy inserts a
/// bundle only of types fixed at compile time. Its mutex is never locked, as a spawn takes the
/// scratch through the world's exclusive access: it makes a resource of the scratch, whose
/// allocator is not `Sync`. Not state.
#[derive(Resource, Default)]
pub struct SpawnParts(Mutex<BundleScratch>);

impl SpawnParts {
    /// Gives `entity`, which spawns, the parts `push` makes from the world and the entity, in
    /// one move; none in a match whose modules declare no part.
    pub fn give(
        entity: &mut EntityWorldMut<'_>,
        push: impl FnOnce(&World, Entity, &mut BundleWriter<'_>),
    ) {
        let Some(mut scratch) = entity.world_scope(SpawnParts::take) else {
            return;
        };
        let mut writer = scratch.writer();
        push(entity.world(), entity.id(), &mut writer);
        if !writer.is_empty() {
            // SAFETY: each part `push` pushed is of a component of the world `entity` is of, as
            // `EntityParts`, a resource of that world, registered each when the modules declared
            // it.
            unsafe { writer.write(entity) };
        }
        entity.world_scope(|world| {
            let mut parts = world.resource_mut::<SpawnParts>();
            *parts.0.get_mut().expect("no spawn panicked holding it") = scratch;
        });
    }

    /// The match's scratch, which it lends a spawn; none in a match with no part.
    fn take(world: &mut World) -> Option<BundleScratch> {
        let mut parts = world.get_resource_mut::<SpawnParts>()?;
        let scratch = parts.0.get_mut().expect("no spawn panicked holding it");
        Some(mem::take(scratch))
    }
}

impl fmt::Debug for SpawnParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SpawnParts")
    }
}
