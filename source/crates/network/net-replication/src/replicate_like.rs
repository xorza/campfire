use bevy_ecs::entity::MapEntities;
use bevy_ecs::prelude::*;
use bevy_ecs::reflect::ReflectMapEntities;
use bevy_reflect::Reflect;
use core::fmt::Debug;

/// A hierarchy member that replicates as its `root`, which each descendant of a `Replicate` gets
/// but one under
/// [`DisableReplicateHierarchy`](crate::disable_replicate_hierarchy::DisableReplicateHierarchy).
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
