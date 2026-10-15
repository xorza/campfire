use bevy_ecs::component::ComponentId;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{DeferredWorld, EntityRef, World};
use campfire_sim::Restoring;

/// The markers of the kinds the modules declare, so an entity's kind is found and an entity of
/// two kinds refused. Derived from the declarations.
#[derive(Resource, Debug, Default)]
pub struct EntityKinds(Vec<ComponentId>);

impl EntityKinds {
    /// Adds the kind of the marker `id`.
    pub fn add(&mut self, id: ComponentId) {
        if !self.0.contains(&id) {
            self.0.push(id);
        }
    }

    /// How many kinds' markers `entity` holds.
    fn held(&self, entity: EntityRef<'_>) -> usize {
        self.0.iter().filter(|&&id| entity.contains_id(id)).count()
    }

    /// Whether `entity` of `world` is of one kind alone, as the restore check of each kind's
    /// state marker asks; a world with no declared kinds holds none to count.
    pub(crate) fn alone(world: &World, entity: Entity) -> bool {
        world
            .get_resource::<EntityKinds>()
            .is_none_or(|kinds| kinds.held(world.entity(entity)) == 1)
    }

    /// The hook of each kind's marker: an entity is of one kind, so a second marker is a logic
    /// error. A restore puts in untrusted values, which its checks refuse with an error, and a
    /// world with no declared kinds, as a test's, holds none to count.
    pub(crate) fn one(world: DeferredWorld<'_>, context: HookContext) {
        if world.contains_resource::<Restoring>() {
            return;
        }
        let Some(kinds) = world.get_resource::<EntityKinds>() else {
            return;
        };
        let held = kinds.held(world.entity(context.entity));
        assert!(
            held == 1,
            "an entity is of one kind, and {:?} holds {held}",
            context.entity
        );
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::component::ComponentId;
    use bevy_ecs::world::EntityRef;

    use crate::entity_kinds::EntityKinds;

    impl EntityKinds {
        /// The marker of `entity`'s kind; none for an entity of no kind.
        pub fn of(&self, entity: EntityRef<'_>) -> Option<ComponentId> {
            self.0.iter().copied().find(|&id| entity.contains_id(id))
        }

        /// The marker of each kind, in the order they were added.
        pub fn markers(&self) -> &[ComponentId] {
            &self.0
        }
    }
}
