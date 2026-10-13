use bevy_ecs::component::ComponentId;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{DeferredWorld, EntityRef, World};
use campfire_sim::Restoring;

/// The markers of the kinds the modules declare, so an entity's kind is found and an entity of
/// two kinds refused. Derived from the declarations.
#[derive(Resource, Debug, Default)]
pub(crate) struct EntityKinds(Vec<ComponentId>);

impl EntityKinds {
    /// Adds the kind of the marker `id`.
    pub(crate) fn add(&mut self, id: ComponentId) {
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

    use crate::units::entity_kinds::EntityKinds;

    impl EntityKinds {
        /// The marker of `entity`'s kind; none for an entity of no kind.
        pub(crate) fn of(&self, entity: EntityRef<'_>) -> Option<ComponentId> {
            self.0.iter().copied().find(|&id| entity.contains_id(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use bevy_ecs::component::ComponentInfo;
    use campfire_sim::HostRule;

    use super::*;
    use crate::release::r#match::Match;
    use crate::units::unit_entity::UnitEntity;

    #[test]
    fn the_unit_group_hosts_exactly_the_kinds_that_require_it() {
        // `UnitEntity`'s host rule lists its kinds by hand; each kind's marker requires it. The
        // two agree: a kind that requires it is in the list, and none other is.
        let game = Match::test(&[], Match::TEST_RATE, None);
        let components = game.components();
        let unit = components.component_id::<UnitEntity>().unwrap();
        let kinds = game.resource::<EntityKinds>();
        let mut requiring: Vec<TypeId> = kinds
            .0
            .iter()
            .filter_map(|&id| components.get_info(id))
            .filter(|info| info.required_components().iter_ids().any(|id| id == unit))
            .filter_map(ComponentInfo::type_id)
            .collect();
        let mut hosted = Vec::new();
        assert!(UnitEntity::kinds(&mut hosted));
        requiring.sort_unstable();
        hosted.sort_unstable();
        assert_eq!(requiring.len(), 2);
        assert_eq!(requiring, hosted);
    }
}
