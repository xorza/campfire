use bevy_ecs::entity::MapEntities;
use bevy_ecs::prelude::*;
use bevy_ecs::reflect::ReflectMapEntities;
use bevy_reflect::Reflect;
use core::fmt::Debug;

/// Marker component that indicates that this entity should be replicated similarly to the entity
/// contained in the component.
///
/// This will be inserted automatically on all children of an entity that has `Replicate`, unless
/// the child has a
/// [`DisableReplicateHierarchy`](crate::disable_replicate_hierarchy::DisableReplicateHierarchy)
/// component.
#[derive(Component, Clone, MapEntities, Copy, Reflect, PartialEq, Eq, Debug)]
#[relationship(relationship_target=ReplicateLikeChildren)]
#[reflect(Component, MapEntities, PartialEq, Debug)]
pub struct ReplicateLike {
    #[entities]
    pub root: Entity,
}

/// Relationship target component associated with [`ReplicateLike`]
#[derive(Component, Debug, Reflect)]
#[relationship_target(relationship=ReplicateLike, linked_spawn)]
#[reflect(Component)]
pub struct ReplicateLikeChildren(Vec<Entity>);
