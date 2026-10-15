use std::alloc::Layout;
use std::any::TypeId;
use std::fmt;

use bevy_ecs::bundle::BundleWriter;
use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::entity::Entity;
use bevy_ecs::ptr::OwningPtr;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_sim::Breach;

use crate::kind_spawn::KindSpawn;
use crate::spawn_parts::SpawnParts;

/// A part as `EntityParts` holds it: its type, how it joins the parts of a spawning entity, and
/// whether an entity as it stands lacks it.
struct Part<H> {
    ty: TypeId,
    push: Box<dyn Fn(&KindSpawn<'_, H>, &mut BundleWriter<'_>) + Send + Sync>,
    missing: Box<dyn Fn(&KindSpawn<'_, H>) -> bool + Send + Sync>,
}

/// The parts the installed modules declare for each entity of the host `H`: the match's, a
/// player's, a unit's, or every unit's, each made at the spawn from the entity and the world.
/// Derived from the declarations: a restore takes the parts from its snapshot, and checks that
/// each entity of the host holds each part its spawn would give it.
#[derive(Resource)]
pub struct EntityParts<H: Send + Sync + 'static> {
    parts: Vec<Part<H>>,
}

impl<H: Send + Sync + 'static> Default for EntityParts<H> {
    fn default() -> EntityParts<H> {
        EntityParts { parts: Vec::new() }
    }
}

impl<H: Component> EntityParts<H> {
    /// Adds the part `start` makes, of the component `id` of the world that holds the parts,
    /// which an entity of the host gets as it spawns, when `start` gives one; the host has one
    /// part of each type.
    pub fn add<C: Component>(
        &mut self,
        id: ComponentId,
        start: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
        let parts = &mut self.parts;
        let ty = TypeId::of::<C>();
        assert!(
            parts.iter().all(|part| part.ty != ty),
            "a host declares a part once"
        );
        parts.push(Part {
            ty,
            push: Box::new(move |at, writer| {
                debug_assert_eq!(
                    at.world().components().component_id::<C>(),
                    Some(id),
                    "a part's component is of the world that holds the parts"
                );
                if at.world().get::<C>(at.entity()).is_some() {
                    return;
                }
                if let Some(part) = start(at) {
                    OwningPtr::make(part, |part| {
                        // SAFETY: `part` holds a `C`, of the component `id`, of the world that
                        // holds these parts, which `SpawnParts` writes the entity of.
                        unsafe { writer.push_component_by_id(id, part, Layout::new::<C>()) };
                    });
                }
            }),
            missing: Box::new(move |at| {
                let held = at.world().get::<C>(at.entity()).is_some();
                !held && start(at).is_some()
            }),
        });
    }

    /// Puts each part the modules declare for the host on `entity`, which spawns, as `push`
    /// makes them, in one move.
    pub fn give(entity: &mut EntityWorldMut<'_>) {
        SpawnParts::give(entity, EntityParts::<H>::push);
    }

    /// Pushes onto `writer` each part the modules declare for the host that `entity` of `world`,
    /// which spawns, lacks, but a part it holds, which its spawn's caller gave; none in a match
    /// whose modules declare none. Each part is made from the entity's core, its facing and its
    /// type's books, never from another declared part, so their order changes none.
    pub fn push(world: &World, entity: Entity, writer: &mut BundleWriter<'_>) {
        let Some(parts) = world.get_resource::<EntityParts<H>>() else {
            return;
        };
        let at = KindSpawn::of(world, entity);
        for part in &parts.parts {
            (part.push)(&at, writer);
        }
    }

    /// The restore check of the host's part `C`: the first entity of the host, by stable id, that
    /// lacks it where its spawn would give it.
    pub fn check<C: Component>(world: &World) -> Result<(), Breach> {
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
