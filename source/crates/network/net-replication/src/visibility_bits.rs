use bevy_ecs::prelude::*;
use bevy_replicon::prelude::ScopeLifetime;
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use bevy_replicon::server::visibility::filters_mask::FilterBit;
use bevy_replicon::server::visibility::registry::FilterRegistry;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;

#[derive(Debug, Resource, Clone, Copy)]
pub(crate) struct VisibilityBits {
    pub(crate) while_visible: FilterBit,
    pub(crate) after_first_visibility: FilterBit,
    pub(crate) always_present: FilterBit,
}

impl VisibilityBits {
    pub(crate) fn iter(self) -> impl Iterator<Item = FilterBit> {
        [
            self.while_visible,
            self.after_first_visibility,
            self.always_present,
        ]
        .into_iter()
    }

    /// Whether `visibility` hides `entity` through any manual bit.
    pub(crate) fn hides(self, visibility: &ClientVisibility, entity: Entity) -> bool {
        let mask = visibility.get(entity);
        self.iter().any(|bit| mask.contains(bit))
    }

    /// Whether `visibility` gives `entity` and `other` the same manual bits.
    pub(crate) fn agree(
        self,
        visibility: &ClientVisibility,
        entity: Entity,
        other: Entity,
    ) -> bool {
        let entity = visibility.get(entity);
        let other = visibility.get(other);
        self.iter()
            .all(|bit| entity.contains(bit) == other.contains(bit))
    }

    /// Gives `entity` the manual bits of `from`. The other filter bits, such as rooms and
    /// replication targets, are evaluated for each entity and stay.
    pub(crate) fn copy(self, visibility: &mut ClientVisibility, entity: Entity, from: Entity) {
        let source = visibility.get(from);
        for bit in self.iter() {
            // `set` takes `visible`, while the mask stores hidden bits.
            visibility.set(entity, bit, !source.contains(bit));
        }
    }
}

impl FromWorld for VisibilityBits {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope(|world, mut filter_registry: Mut<'_, FilterRegistry>| {
            world.resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
                let mut register = |lifetime| {
                    filter_registry.register_scope::<Entity>(world, &mut registry, lifetime)
                };
                Self {
                    while_visible: register(ScopeLifetime::WhileVisible),
                    after_first_visibility: register(ScopeLifetime::AfterFirstVisibility),
                    always_present: register(ScopeLifetime::AlwaysPresent),
                }
            })
        })
    }
}
