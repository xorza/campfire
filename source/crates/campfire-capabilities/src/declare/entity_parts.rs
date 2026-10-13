use std::any::TypeId;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_sim::Breach;

use crate::declare::kind_spawn::KindSpawn;

/// A part as `EntityParts` holds it: its type, how it goes on a spawning entity, and whether an
/// entity as it stands lacks it.
struct Part<H> {
    ty: TypeId,
    give: Box<dyn Fn(&mut EntityWorldMut<'_>) + Send + Sync>,
    missing: Box<dyn Fn(&KindSpawn<'_, H>) -> bool + Send + Sync>,
}

/// The parts the installed modules declare for each entity of the host `H`: the match's, a
/// player's, a unit's, or every unit's, each made at the spawn from the entity and the world.
/// Derived from the declarations: a restore takes the parts from its snapshot, and checks that
/// each entity of the host holds each part its spawn would give it.
#[derive(Resource)]
pub(crate) struct EntityParts<H: Send + Sync + 'static> {
    parts: Arc<Vec<Part<H>>>,
    host: PhantomData<H>,
}

impl<H: Send + Sync + 'static> Default for EntityParts<H> {
    fn default() -> EntityParts<H> {
        EntityParts {
            parts: Arc::default(),
            host: PhantomData,
        }
    }
}

impl<H: Component> EntityParts<H> {
    /// Adds the part `start` makes, which an entity of the host gets as it spawns, when `start`
    /// gives one; the host has one part of each type.
    pub(crate) fn add<C: Component>(&mut self, start: fn(&KindSpawn<'_, H>) -> Option<C>) {
        let parts = Arc::get_mut(&mut self.parts).expect("the install adds parts before a spawn");
        let ty = TypeId::of::<C>();
        assert!(
            parts.iter().all(|part| part.ty != ty),
            "a host declares a part once"
        );
        parts.push(Part {
            ty,
            give: Box::new(move |entity| {
                let at = KindSpawn::of(entity.world(), entity.id());
                if let Some(part) = start(&at) {
                    entity.insert_if_new(part);
                }
            }),
            missing: Box::new(move |at| {
                let held = at.world().get::<C>(at.entity()).is_some();
                !held && start(at).is_some()
            }),
        });
    }

    /// Puts each part the modules declare for the host on `entity`, which spawns, in the order
    /// they declared them, but a part it holds, which its spawn's caller gave; none in a match
    /// whose modules declare none.
    pub(crate) fn give(entity: &mut EntityWorldMut<'_>) {
        let Some(parts) = entity.world().get_resource::<EntityParts<H>>() else {
            return;
        };
        let parts = Arc::clone(&parts.parts);
        for part in parts.iter() {
            (part.give)(entity);
        }
    }

    /// The restore check of the host's part `C`: the first entity of the host, by stable id, that
    /// lacks it where its spawn would give it.
    pub(crate) fn check<C: Component>(world: &World) -> Result<(), Breach> {
        let Some(parts) = world.get_resource::<EntityParts<H>>() else {
            return Ok(());
        };
        let part = parts.parts.iter().find(|part| part.ty == TypeId::of::<C>());
        let part = part.expect("the check of a declared part");
        let lacks = |entity: Entity| {
            world.get::<H>(entity).is_some() && (part.missing)(&KindSpawn::of(world, entity))
        };
        Breach::first(world, lacks)
    }
}

impl<H> fmt::Debug for Part<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Part")
    }
}

impl<H: Send + Sync + 'static> fmt::Debug for EntityParts<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EntityParts")
            .field(&self.parts.len())
            .finish()
    }
}
